//! Pagination math, header and footer content, PDF structure, the
//! export-pdf command, persistence per layout, and headless renders of the
//! Page Setup tabs and a preview page for review.
use super::ops::Op;
use super::paginate::{self, Area, Extent, RowSlot};
use super::setup::{self, Content, IncludeOn, Orientation, PageSetup, Scaling};
use super::*;
use crate::headless_render::SoftwarePainter;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn unique(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openxer-print-{name}-{}-{nanos}",
        std::process::id()
    ))
}

pub(super) fn app(name: &str) -> (PathBuf, egui::Context, App) {
    let path = unique(name).with_extension("db");
    let project = scheduled_starter();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let app = App::new(&ctx, store, project);
    (path, ctx, app)
}

/// Contents of every stream in a PDF, inflated.
fn streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![];
    let mut rest = pdf;
    while let Some(start) = find(rest, b"stream\n") {
        let body = &rest[start + 7..];
        let end = find(body, b"\nendstream").expect("stream ends");
        let data = &body[..end];
        out.push(
            miniz_oxide::inflate::decompress_to_vec_zlib(data).unwrap_or_else(|_| data.to_vec()),
        );
        rest = &body[end + 10..];
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

/// The numbers after the first `/MediaBox [`.
fn media_box(pdf: &[u8]) -> Vec<f32> {
    let at = find(pdf, b"/MediaBox [").unwrap() + 11;
    let end = at + find(&pdf[at..], b"]").unwrap();
    std::str::from_utf8(&pdf[at..end])
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

// ------------------------------------------------------------ pagination

const LETTER_LANDSCAPE: Area = Area {
    width: 10. * 96.,
    height: 7.5 * 96.,
};

#[test]
fn adjust_scales_text_and_keeps_the_timescale() {
    let extent = Extent {
        table_width: 600.,
        span_days: 100.,
        header_height: 35.,
        body_height: 900.,
    };
    let plan = paginate::plan(
        Scaling::Adjust { percent: 50 },
        LETTER_LANDSCAPE,
        extent,
        8.,
    );
    assert_eq!(plan.scale, 0.5);
    assert_eq!(plan.pixels_per_day, 8.);
}

#[test]
fn fit_to_pages_wide_shrinks_wide_content_and_stretches_narrow_timescales() {
    let wide = Extent {
        table_width: 600.,
        span_days: 300.,
        header_height: 35.,
        body_height: 900.,
    };
    // 600 + 300 * 8 = 3000 px into one 960 px page.
    let plan = paginate::plan(
        Scaling::Fit { wide: 1, tall: 0 },
        LETTER_LANDSCAPE,
        wide,
        8.,
    );
    assert!((plan.scale - 960. / 3000.).abs() < 1e-6);
    assert_eq!(plan.pixels_per_day, 8.);
    assert_eq!(paginate::columns(3000., plan.scale, LETTER_LANDSCAPE), 1);
    // Two pages wide doubles the scale; two across.
    let plan = paginate::plan(
        Scaling::Fit { wide: 2, tall: 0 },
        LETTER_LANDSCAPE,
        wide,
        8.,
    );
    assert!((plan.scale - 1920. / 3000.).abs() < 1e-6);
    assert_eq!(paginate::columns(3000., plan.scale, LETTER_LANDSCAPE), 2);
    // A tall limit wins when it is tighter: 1 page tall holds 720 px.
    let plan = paginate::plan(
        Scaling::Fit { wide: 1, tall: 1 },
        LETTER_LANDSCAPE,
        Extent {
            body_height: 4000.,
            ..wide
        },
        8.,
    );
    assert!((plan.scale - 720. / 4035.).abs() < 1e-6);
    // Narrow content keeps 100% text and fills the width with timescale.
    let narrow = Extent {
        span_days: 20.,
        ..wide
    };
    let plan = paginate::plan(
        Scaling::Fit { wide: 1, tall: 0 },
        LETTER_LANDSCAPE,
        narrow,
        8.,
    );
    assert_eq!(plan.scale, 1.);
    assert!((600. + 20. * plan.pixels_per_day - 960.).abs() < 1e-3);
}

#[test]
fn fit_timescale_spreads_the_chart_over_pages_wide_at_normal_size() {
    let extent = Extent {
        table_width: 400.,
        span_days: 200.,
        header_height: 35.,
        body_height: 500.,
    };
    let plan = paginate::plan(
        Scaling::FitTimescale { wide: 3 },
        LETTER_LANDSCAPE,
        extent,
        8.,
    );
    assert_eq!(plan.scale, 1.);
    assert!((400. + 200. * plan.pixels_per_day - 3. * 960.).abs() < 1e-2);
    assert_eq!(paginate::columns(3. * 960., 1., LETTER_LANDSCAPE), 3);
}

#[test]
fn row_blocks_break_between_rows_and_on_group_bands() {
    let rows: Vec<RowSlot> = (0..10)
        .map(|k| RowSlot {
            top: k as f32 * 18.,
            height: 18.,
            break_before: k == 7,
            ..RowSlot::default()
        })
        .collect();
    // 50 px pages hold two rows.
    let blocks = paginate::blocks(&rows, 180., 50.);
    assert_eq!(
        blocks,
        vec![
            (0., 36.),
            (36., 72.),
            (72., 108.),
            (108., 126.),
            (126., 162.),
            (162., 180.)
        ],
        "rows stay whole; row 7 starts a page"
    );
    // A row taller than a page is cut into page-high pieces.
    let tall = [RowSlot {
        top: 0.,
        height: 130.,
        ..RowSlot::default()
    }];
    assert_eq!(
        paginate::blocks(&tall, 130., 50.),
        vec![(0., 50.), (50., 100.), (100., 130.)]
    );
    // Tiles run across, then down; the header repeats on each page.
    let tiles = paginate::tiles(
        1900.,
        35.,
        &rows,
        180.,
        1.,
        Area {
            width: 1000.,
            height: 35. + 50.,
        },
    )
    .unwrap();
    assert_eq!(tiles.len(), 6 * 2);
    assert_eq!(
        (tiles[0].column, tiles[1].column, tiles[2].block),
        (0, 1, 1)
    );
    assert_eq!((tiles[1].x0, tiles[1].x1), (1000., 1900.));
}

#[test]
fn margins_and_bands_shrink_the_printable_area() {
    let (path, _ctx, mut app) = app("margins");
    let mut s = PageSetup {
        scaling: Scaling::Adjust { percent: 100 },
        orientation: Orientation::Landscape,
        allow_tiling: true,
        ..PageSetup::default()
    };
    s.header.include_on = IncludeOn::NoPages;
    s.footer.include_on = IncludeOn::NoPages;
    let wide = app.print_with(&s).unwrap();
    s.margins.left = 2.;
    s.margins.right = 2.;
    let narrow = app.print_with(&s).unwrap();
    assert!(
        narrow.pages.len() > wide.pages.len(),
        "wider margins need more pages"
    );
    assert!(narrow.across > wide.across);
    // Every page is Letter landscape at 96 px per inch.
    assert!(narrow
        .pages
        .iter()
        .all(|p| p.size == Vec2::new(1056., 816.)));
    // Nothing is drawn into the side margins except clipped ops.
    for page in &narrow.pages {
        for c in &page.ops {
            assert!(
                c.clip.left() >= 2. * 96. - 1.5 && c.clip.right() <= 1056. - 2. * 96. + 1.5,
                "{:?}",
                c.clip
            );
        }
    }
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ setup model

#[test]
fn fields_expand_and_timescale_expressions_resolve() {
    let text = setup::expand(
        "[project_name] p[page_number]/[total_pages] [nope]",
        |t| match t {
            "project_name" => Some("Riverbend".into()),
            "page_number" => Some("2".into()),
            "total_pages" => Some("5".into()),
            _ => None,
        },
    );
    assert_eq!(text, "Riverbend p2/5 [nope]");
    assert_eq!(setup::expand("a [b", |_| None), "a [b");
    let no_date = |_: &str| None;
    let e = setup::parse_expr("DD-1W", no_date).unwrap();
    assert_eq!(
        e,
        setup::DateExpr::Anchored {
            anchor: setup::Anchor::DataDate,
            offset: -1,
            unit: 'W'
        }
    );
    let at = |a: setup::Anchor| match a {
        setup::Anchor::DataDate => 100 * DAY,
        setup::Anchor::ProjectFinish => 200 * DAY,
        _ => 0,
    };
    assert_eq!(e.resolve(at), Ok(93 * DAY));
    assert_eq!(
        setup::parse_expr("pf + 2d", no_date).unwrap().resolve(at),
        Ok(202 * DAY)
    );
    assert_eq!(setup::parse_expr("PS", no_date).unwrap().resolve(at), Ok(0));
    assert!(setup::parse_expr("DD*2", no_date).is_err());
    assert!(setup::parse_expr("tomorrow", no_date).is_err());
    assert_eq!(
        setup::parse_expr("1-Jan-26", |_| Some(42)).unwrap(),
        setup::DateExpr::Fixed(42)
    );
}

#[test]
fn sections_divide_and_dividers_move_within_bounds() {
    let mut band = PageSetup::default().header;
    band.divide(5);
    assert_eq!(band.count(), 5);
    assert_eq!(band.widths, vec![20.; 5]);
    band.divide(9);
    assert_eq!(band.count(), setup::MAX_SECTIONS);
    band.divide(2);
    band.move_divider(0, 0.75);
    assert_eq!(band.widths, vec![75., 25.]);
    band.move_divider(0, 0.99);
    assert_eq!(band.widths, vec![95., 5.], "sections keep 5%");
    let edges = band.edges();
    assert_eq!((edges[0], edges[2]), (0., 1.));
    assert!(IncludeOn::FirstPage.on(0, 3) && !IncludeOn::FirstPage.on(1, 3));
    assert!(IncludeOn::LastPage.on(2, 3) && !IncludeOn::LastPage.on(0, 3));
    assert!(!IncludeOn::NoPages.on(0, 1));
    let mut s = PageSetup {
        scaling: Scaling::Fit { wide: 0, tall: 999 },
        ..PageSetup::default()
    };
    s.margins.top = 9.;
    s.sanitize();
    assert_eq!(s.scaling, Scaling::Fit { wide: 1, tall: 200 });
    assert!(s.margins.top <= 8.5 / 3.);
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<PageSetup>(&json).unwrap(), s);
}

#[test]
fn page_setup_persists_per_layout() {
    let (path, _ctx, mut app) = app("persist");
    let s = PageSetup {
        paper: "A3".into(),
        orientation: Orientation::Portrait,
        ..PageSetup::default()
    };
    app.save_page_setup(&s).unwrap();
    assert_eq!(app.page_setup().paper, "A3");
    let first = app.grid.layout.name.clone();
    app.grid.layout.name = "Another Layout".into();
    // A new layout starts from the defaults, with the paper chosen last.
    assert_eq!(
        app.page_setup(),
        PageSetup {
            paper: "A3".into(),
            ..PageSetup::default()
        },
        "a new layout starts from the defaults"
    );
    app.grid.layout.name = first;
    assert_eq!(app.page_setup().orientation, Orientation::Portrait);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ content

#[test]
fn header_and_footer_show_fields_legend_and_revisions_on_the_chosen_pages() {
    let (path, _ctx, mut app) = app("bands");
    let mut s = PageSetup {
        scaling: Scaling::Adjust { percent: 100 },
        allow_tiling: true,
        ..PageSetup::default()
    };
    s.footer.sections[1] = setup::Section::with(Content::Revisions);
    s.footer.sections[1].revisions.push(setup::Revision {
        date: "04-Nov-26".into(),
        revision: "Issued for review".into(),
        checked: "CL".into(),
        approved: "OK".into(),
    });
    s.footer.height = 1.;
    let printed = app.print_with(&s).unwrap();
    let n = printed.pages.len();
    assert!(n >= 2, "the starter project spans pages at 100%: {n}");
    let all = |k: usize| printed.pages[k].texts().join("\n");
    let first = all(0);
    let name = views::project_title(&app.project);
    assert!(first.contains(&name), "project name in the header: {first}");
    assert!(first.contains(&app.grid.layout.name));
    assert!(first.contains(&format!("Page 1 of {n}")));
    assert!(all(n - 1).contains(&format!("Page {n} of {n}")));
    for bar in ["Remaining Work", "Critical Remaining Work", "Milestone"] {
        assert!(first.contains(bar), "legend entry {bar}");
    }
    for word in ["Revisions", "Date", "Approved", "Issued for review"] {
        assert!(first.contains(word), "revision box {word}");
    }
    let dd = format::date(
        app.project.model.origin_serial,
        app.project.model.project.data_date,
    );
    assert!(first.contains(&dd), "data date field");
    // Header on the first page only, footer on none.
    s.header.include_on = IncludeOn::FirstPage;
    s.header.sections[1].text = "HEADER-MARK".into();
    s.footer.include_on = IncludeOn::NoPages;
    let printed = app.print_with(&s).unwrap();
    let texts: Vec<String> = printed.pages.iter().map(|p| p.texts().join("\n")).collect();
    assert!(texts[0].contains("HEADER-MARK"));
    assert!(texts.iter().skip(1).all(|t| !t.contains("HEADER-MARK")));
    assert!(texts.iter().all(|t| !t.contains("Page 1 of")));
    let _ = std::fs::remove_file(path);
}

#[test]
fn printed_canvas_holds_the_table_text_bars_and_options() {
    let (path, _ctx, mut app) = app("canvas");
    let s = PageSetup::default();
    let printed = app.print_with(&s).unwrap();
    assert_eq!(
        printed.pages.len(),
        1,
        "Fit to 1 page wide fits the starter project"
    );
    let page = &printed.pages[0];
    let texts = page.texts().join("\n");
    for activity in &app.project.model.activities {
        let id = &activity.task_code;
        assert!(texts.contains(id.as_str()), "activity {id} printed");
    }
    let fills = |color: Color32| {
        page.ops
            .iter()
            .filter(|c| matches!(&c.op, Op::Fill { color: k, .. } if *k == color))
            .count()
    };
    assert!(
        fills(crate::theme::color(crate::theme::CRITICAL)) > 0,
        "critical bars"
    );
    assert!(
        fills(Color32::from_rgb(0xC0, 0xC0, 0xC0)) > 20,
        "grid lines"
    );
    assert!(printed.timescale.is_some());
    // Without grid lines and the Gantt: no bars, fewer lines.
    let mut s = PageSetup::default();
    s.options.grid_lines = false;
    s.options.gantt_chart = false;
    // The footer's legend draws bars of its own.
    s.footer.include_on = IncludeOn::NoPages;
    let table = app.print_with(&s).unwrap();
    let p = &table.pages[0];
    let red = p
        .ops
        .iter()
        .filter(|c| matches!(&c.op, Op::Fill { color, .. } if *color == crate::theme::color(crate::theme::CRITICAL)))
        .count();
    assert_eq!(red, 0);
    assert!(table.timescale.is_none());
    // Both parts off is refused.
    s.options.activity_table = false;
    assert!(app.print_with(&s).is_err());
    // The app's view state is untouched by printing.
    let before = (
        app.timeline_origin,
        app.pixels_per_day,
        app.gantt.options.show_legend,
    );
    let _ = app.print_with(&PageSetup::default()).unwrap();
    assert_eq!(
        before,
        (
            app.timeline_origin,
            app.pixels_per_day,
            app.gantt.options.show_legend
        )
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn network_on_top_prints_activity_boxes() {
    let (path, _ctx, mut app) = app("network");
    app.set_top(panes::TopView::Network);
    let printed = app.print_with(&PageSetup::default()).unwrap();
    let texts = printed.pages[0].texts().join("\n");
    let id = &app.project.model.activities[0].task_code;
    assert!(texts.contains(id.as_str()));
    assert!(
        texts.contains("Critical activity"),
        "network legend in the footer"
    );
    assert!(printed.timescale.is_none());
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ PDF

#[test]
fn pdf_has_the_pages_sizes_fonts_and_text() {
    let (path, _ctx, mut app) = app("pdf");
    let mut s = PageSetup {
        scaling: Scaling::Adjust { percent: 100 },
        orientation: Orientation::Landscape,
        allow_tiling: true,
        ..PageSetup::default()
    };
    let printed = app.print_with(&s).unwrap();
    let bytes = pdf::write(&printed.pages, "Test");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.ends_with(b"%%EOF") || bytes.ends_with(b"%%EOF\n"));
    let n = printed.pages.len();
    assert!(n > 1);
    assert_eq!(
        count(&bytes, b"/Type /Page"),
        n + 1,
        "n pages and the page tree"
    );
    assert!(find(&bytes, format!("/Count {n}").as_bytes()).is_some());
    assert_eq!(
        media_box(&bytes),
        vec![0., 0., 792., 612.],
        "Letter landscape in points"
    );
    assert!(find(&bytes, b"/BaseFont /Helvetica").is_some());
    assert!(find(&bytes, b"/BaseFont /Helvetica-Bold").is_some());
    let content: Vec<u8> = streams(&bytes).concat();
    let name = app.project.name(0).to_string();
    assert!(
        find(&content, format!("({name}) Tj").as_bytes()).is_some(),
        "activity name as text"
    );
    assert!(
        find(&content, b" re\n").is_some() || find(&content, b" re").is_some(),
        "vector rectangles"
    );
    assert!(find(&content, b"Page 1 of").is_some());
    // Portrait A4 page size.
    s.paper = "A4".into();
    s.orientation = Orientation::Portrait;
    let printed = app.print_with(&s).unwrap();
    let bytes = pdf::write(&printed.pages, "Test");
    let mb = media_box(&bytes);
    assert!(
        (mb[2] - 595.3).abs() < 0.5 && (mb[3] - 841.9).abs() < 0.5,
        "{mb:?}"
    );
    assert_eq!(
        pdf::win_ansi("A\u{2013}\u{e9}\u{20ac}?"),
        vec![b'A', 0x96, 0xE9, 0x80, b'?']
    );
    // Every character of code page 1252 prints as itself; others print `?`.
    assert_eq!(
        pdf::win_ansi("\u{160}\u{161}\u{152}\u{153}\u{17d}\u{17e}\u{178}\u{2030}\u{2020}\u{201e}"),
        vec![0x8A, 0x9A, 0x8C, 0x9C, 0x8E, 0x9E, 0x9F, 0x89, 0x86, 0x84]
    );
    assert_eq!(pdf::win_ansi("\u{142}\u{3b1}"), vec![b'?', b'?']);
    // Writing to a file goes through the same path.
    let out = unique("written").with_extension("pdf");
    let pages = app.write_pdf(&out, &PageSetup::default()).unwrap();
    assert_eq!(pages, 1);
    assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));
    let _ = std::fs::remove_file(out);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ CLI

#[test]
fn export_pdf_command_prints_an_xer_and_a_workspace_layout() {
    let dir = unique("cli");
    std::fs::create_dir_all(&dir).unwrap();
    let xer = dir.join("starter.xer");
    std::fs::write(&xer, scheduled_starter().updated_document().text()).unwrap();
    let out = dir.join("starter.pdf");
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let summary = cli::run(&args(&[
        "--input",
        xer.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--paper",
        "tabloid",
    ]))
    .unwrap();
    assert_eq!(summary["pages"], 1);
    assert_eq!(summary["paper"], "Tabloid");
    assert_eq!(summary["page_inches"][0], 17.);
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(media_box(&bytes), vec![0., 0., 1224., 792.]);
    // Existing outputs are refused; bad flags are named.
    let again = cli::run(&args(&[
        "--input",
        xer.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]));
    assert!(again.unwrap_err().contains("already exists"));
    assert!(cli::run(&args(&["--output", "x.pdf"])).is_err());
    assert!(cli::run(&args(&["--bogus", "1"]))
        .unwrap_err()
        .contains("Unknown flag"));
    // A workspace with a saved layout and its own page setup.
    let (db, _ctx, mut app) = app("cli-db");
    app.save_layout_as("Print Test").unwrap();
    let s = PageSetup {
        paper: "Legal".into(),
        orientation: Orientation::Portrait,
        ..PageSetup::default()
    };
    app.save_page_setup(&s).unwrap();
    app.open_layout(layout_standard(), true).unwrap();
    drop(app);
    let out2 = dir.join("layout.pdf");
    let summary = cli::run(&args(&[
        "--database",
        db.to_str().unwrap(),
        "--output",
        out2.to_str().unwrap(),
        "--layout",
        "Print Test",
        "--view",
        "network",
    ]))
    .unwrap();
    assert_eq!(summary["layout"], "Print Test");
    assert_eq!(summary["paper"], "Legal");
    assert_eq!(summary["view"], "network");
    assert_eq!(
        media_box(&std::fs::read(&out2).unwrap()),
        vec![0., 0., 612., 1008.]
    );
    let missing = cli::run(&args(&[
        "--database",
        db.to_str().unwrap(),
        "--output",
        dir.join("none.pdf").to_str().unwrap(),
        "--layout",
        "No Such Layout",
    ]));
    assert!(missing.unwrap_err().contains("No Such Layout"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(db);
}

fn layout_standard() -> &'static str {
    crate::grid::layout::STANDARD_NAME
}

// ------------------------------------------------------------ renders

/// Where each Page Setup control sits, in client pixels (x, y, height):
/// action name, tab.
const CONTROLS: [(&str, usize, f32, f32, f32); 31] = [
    ("page_setup_ok", 0, 632., 6., 25.),
    ("page_setup_cancel", 0, 632., 36., 25.),
    ("page_setup_apply", 0, 632., 76., 25.),
    ("page_setup_default", 0, 632., 116., 25.),
    ("page_setup_help", 0, 632., 156., 25.),
    ("page_setup_portrait", 0, 71., 74., 17.),
    ("page_setup_landscape", 0, 217., 74., 17.),
    ("page_setup_scaling_0", 0, 21., 163., 17.),
    ("page_setup_scaling_1", 0, 21., 190., 17.),
    ("page_setup_scaling_2", 0, 21., 217., 17.),
    ("page_setup_percent", 0, 193., 160., 23.),
    ("page_setup_wide", 0, 193., 187., 23.),
    ("page_setup_tall", 0, 354., 187., 23.),
    ("page_setup_timescale_wide", 0, 193., 214., 23.),
    ("page_setup_paper", 0, 126., 305., 22.),
    ("page_setup_width", 0, 126., 333., 23.),
    ("page_setup_height", 0, 254., 333., 23.),
    ("page_setup_margin_0", 1, 171., 48., 23.),
    ("page_setup_margin_1", 1, 18., 144., 23.),
    ("page_setup_margin_2", 1, 314., 144., 23.),
    ("page_setup_margin_3", 1, 171., 257., 23.),
    ("page_setup_header_divide", 2, 93., 42., 23.),
    ("page_setup_header_include", 2, 287., 42., 22.),
    ("page_setup_header_height", 2, 514., 42., 23.),
    ("page_setup_header_dividers", 2, 24., 99., 17.),
    ("page_setup_header_modify", 2, 28., 361., 21.),
    ("page_setup_ts_start", 4, 206., 38., 21.),
    ("page_setup_ts_finish", 4, 205., 70., 21.),
    ("page_setup_print_0", 4, 38., 141., 17.),
    ("page_setup_print_3", 4, 38., 213., 17.),
    ("page_setup_break", 4, 39., 363., 17.),
];

/// Renders the five Page Setup tabs and a preview page to the ignored
/// `artifacts/review/print/`, checks each control's rectangle against
/// [`CONTROLS`] and writes the differences to `compare.json`.
#[test]
fn render_page_setup_tabs_and_preview_for_review() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/print");
    std::fs::create_dir_all(&out).unwrap();
    let (path, ctx, mut app) = app("render");
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    // File > Page Setup through the menu.
    painter.click_action(&mut app, &ctx, &mut time, "menu_bar_file");
    painter.click_action(&mut app, &ctx, &mut time, "menu_page_setup");
    assert!(app.print.dialog.is_some(), "the menu opens Page Setup");
    let mut report = vec![];
    let mut within = 0;
    for (tab, name) in dialog::TABS.iter().enumerate() {
        if tab > 0 {
            painter.click_action(&mut app, &ctx, &mut time, &format!("page_setup_tab_{tab}"));
        }
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        assert_eq!(app.print.dialog.as_ref().unwrap().tab, tab);
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        let close = app.action_rects["page_setup_title_close"];
        let size = dialog::CLIENT + Vec2::new(2., 33.);
        let left = (close[2] + 1. - size.x).round() as u32;
        let top = (close[1] - 1.).round() as u32;
        let crop =
            image::imageops::crop_imm(&image, left, top, size.x as u32, size.y as u32).to_image();
        let file = format!("page-setup-{}.png", name.to_ascii_lowercase());
        crop.save(out.join(&file)).unwrap();
        let client = Pos2::new(left as f32 + 1., top as f32 + 32.);
        for (action, _, x, y, h) in CONTROLS.iter().filter(|r| r.1 == tab) {
            let r = app
                .action_rects
                .get(*action)
                .unwrap_or_else(|| panic!("{action} drawn"));
            let dx = r[0] - client.x - x;
            let dy = r[1] - client.y - y;
            let dh = r[3] - r[1] - h;
            let ok = dx.abs() <= 1. && dy.abs() <= 1. && dh.abs() <= 1.;
            within += usize::from(ok);
            report.push(
                serde_json::json!({"control": action, "dx": dx, "dy": dy, "dh": dh, "within": ok}),
            );
        }
    }
    let checked = CONTROLS.len();
    assert_eq!(within, checked, "{report:#?}");
    // Apply stays disabled until something changes; OK saves and closes.
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_tab_0");
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_portrait");
    assert_eq!(
        app.print.dialog.as_ref().unwrap().draft.orientation,
        Orientation::Portrait
    );
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_ok");
    assert!(app.print.dialog.is_none());
    assert_eq!(app.page_setup().orientation, Orientation::Portrait);
    // Print Preview from the toolbar, then a page render.
    painter.click_action(&mut app, &ctx, &mut time, "tool_print_preview");
    assert!(app.print.preview.is_some(), "{:?}", app.error);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("print-preview.png")).unwrap();
    let pages = app.print.preview.as_ref().unwrap().count();
    if pages > 1 {
        painter.click_action(&mut app, &ctx, &mut time, "preview_right");
        assert_eq!(app.print.preview.as_ref().unwrap().page, 1);
    }
    painter.click_action(&mut app, &ctx, &mut time, "preview_zoom_in");
    assert!(app.print.preview.as_ref().unwrap().zoom.is_some());
    painter.click_action(&mut app, &ctx, &mut time, "preview_close");
    assert!(app.print.preview.is_none());
    std::fs::write(
        out.join("compare.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"controls_within_1px": within, "controls": checked, "details": report})).unwrap(),
    )
    .unwrap();
    // A PDF of the same layout for a look.
    app.write_pdf(&out.join("starter.pdf"), &app.page_setup())
        .unwrap();
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ hardening

#[test]
fn malformed_and_huge_timescale_expressions_are_errors_not_panics() {
    let no_date = |_: &str| None;
    for bad in [
        "DD\u{e9}5d",
        "PS\u{e9}5d",
        "PF+\u{e9}5",
        "DD+5\u{e9}",
        "CD+",
        "PS-W",
        "DD+1.5W",
        "PF++1D",
        "DD+99999999999999999999999D",
        "PS+9223372036854775807Y",
        "DD-1000Y",
        "PF+18301D",
    ] {
        assert!(setup::parse_expr(bad, no_date).is_err(), "{bad:?}");
    }
    assert!(setup::parse_expr("PF+50Y", no_date).is_ok());
    // Overflow while resolving is an error too.
    let far = setup::DateExpr::Anchored {
        anchor: setup::Anchor::DataDate,
        offset: i64::MAX / 2,
        unit: 'Y',
    };
    assert!(far.resolve(|_| 0).is_err());
    let edge = setup::DateExpr::Anchored {
        anchor: setup::Anchor::DataDate,
        offset: 1,
        unit: 'D',
    };
    assert!(edge.resolve(|_| i64::MAX).is_err());
    // A stored setup with a damaged expression prints an error message.
    let (path, _ctx, mut app) = app("hardening");
    let mut s = PageSetup::default();
    s.options.timescale_start = "DD\u{e9}5d".into();
    assert!(app.print_with(&s).is_err());
    s.options.timescale_start = "PS-50Y".into();
    s.options.timescale_finish = "PF+50Y".into();
    assert!(app.print_with(&s).unwrap_err().contains("50 years"));
    // A job past the page limit is refused before painting.
    s.options.timescale_start = "PS-49Y".into();
    s.options.timescale_finish = "PF+1W".into();
    s.scaling = Scaling::Adjust { percent: 400 };
    s.allow_tiling = true;
    s.orientation = Orientation::Portrait;
    s.margins.left = 2.5;
    s.margins.right = 2.5;
    assert!(app.print_with(&s).unwrap_err().contains("limit is 2000"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn page_limits_hold_in_the_math() {
    let area = LETTER_LANDSCAPE;
    assert!(paginate::check_pages(960. * 2001., 35., 10., 1., area).is_err());
    assert!(paginate::check_pages(960. * 10., 35., 500., 1., area).is_ok());
    let rows = [RowSlot {
        top: 0.,
        height: 1.0e7,
        ..RowSlot::default()
    }];
    assert!(paginate::tiles(100., 0., &rows, 1.0e7, 1., area).is_err());
}

#[test]
fn print_files_are_created_new_and_never_overwrite() {
    let dir = unique("printdir");
    let first = super::write_new(&dir, b"%PDF-1").unwrap();
    let second = super::write_new(&dir, b"%PDF-2").unwrap();
    assert_ne!(first, second, "names are not predictable");
    assert_eq!(std::fs::read(&first).unwrap(), b"%PDF-1");
    let name = first.file_name().unwrap().to_str().unwrap().to_string();
    assert!(name.starts_with("OpenXER-print-"));
    // An existing file at the export path is refused and left alone.
    let xer = dir.join("starter.xer");
    std::fs::write(&xer, scheduled_starter().updated_document().text()).unwrap();
    let out = dir.join("taken.pdf");
    std::fs::write(&out, b"keep me").unwrap();
    let args: Vec<String> = [
        "--input",
        xer.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(cli::run(&args).unwrap_err().contains("already exists"));
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ resources

pub(super) fn resource_app(name: &str) -> (PathBuf, egui::Context, App) {
    let path = unique(name).with_extension("db");
    let project = crate::views::resources::tests::with_resource();
    let mut store = Store::open(path.clone()).unwrap();
    store.save(&project).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let app = App::new(&ctx, store, project);
    (path, ctx, app)
}

/// Print, write the PDF, check its structure and that a second run gives
/// the same pages. Returns the pages' text.
fn print_checked(app: &mut App, s: &PageSetup) -> (usize, String) {
    let printed = app.print_with(s).unwrap();
    let n = printed.pages.len();
    let bytes = pdf::write(&printed.pages, "Test");
    assert_eq!(pdf::check(&bytes), Ok(n), "PDF structure");
    let again = app.print_with(s).unwrap();
    assert_eq!(again.pages.len(), n, "page count is stable");
    let texts = |p: &Printed| {
        p.pages
            .iter()
            .map(|p| p.texts().join("\n"))
            .collect::<Vec<_>>()
    };
    assert_eq!(texts(&printed), texts(&again), "pages are stable");
    (n, texts(&printed).join("\n"))
}

#[test]
fn bottom_profile_spreadsheet_and_trace_logic_print_under_the_table_and_gantt() {
    use panes::BottomView as B;
    let (path, _ctx, mut app) = resource_app("bottom");
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    app.selected = Some(i);
    app.panes.usage.resource = Some(7);
    let s = PageSetup::default();
    app.set_bottom(Some(B::Details));
    let (plain, plain_text) = print_checked(&mut app, &s);
    assert!(app.printable_bottom().is_none());
    for (view, kind, marker) in [
        (B::ActivityUsageProfile, 0, "Budgeted Labor"),
        (B::ResourceUsageProfile, 0, "Remaining Early Units"),
        (B::ActivityUsageSheet, 1, "Labor Units"),
        (B::ResourceUsageSheet, 1, "PIPE"),
        (B::TraceLogic, 2, ""),
    ] {
        app.set_bottom(Some(view));
        assert_eq!(app.printable_bottom(), Some((kind, view)));
        let (pages, text) = print_checked(&mut app, &s);
        assert!(pages >= plain, "{view:?}");
        assert!(text.len() > plain_text.len(), "{view:?} adds to the print");
        if !marker.is_empty() {
            assert!(text.contains(marker), "{view:?} prints {marker}: {text}");
            assert!(!plain_text.contains(marker));
        }
        // The Page Setup option leaves it out.
        let mut off = s.clone();
        match kind {
            0 => off.options.profile = false,
            1 => off.options.spreadsheet = false,
            _ => off.options.trace_logic = false,
        }
        let (n, text) = print_checked(&mut app, &off);
        assert_eq!(n, plain, "{view:?} off");
        assert_eq!(text, plain_text, "{view:?} off");
    }
    // Trace Logic starts a page of its own with the traced boxes.
    app.set_bottom(Some(B::TraceLogic));
    let printed = app.print_with(&s).unwrap();
    assert!(printed.pages.len() > plain);
    let last = printed.pages.last().unwrap().texts().join("\n");
    let code = &app.project.model.activities[i].task_code;
    assert!(last.contains(code.as_str()), "the focus box prints");
    // The table's column header repeats on the table and Gantt pages but
    // not on the Trace Logic page, which has no table columns.
    let page_texts: Vec<String> = printed.pages.iter().map(|p| p.texts().join("\n")).collect();
    for (k, text) in page_texts.iter().enumerate() {
        assert_eq!(
            text.contains("Activity ID"),
            k < plain,
            "page {k} of {}: the column header belongs to the table pages only",
            page_texts.len()
        );
    }
    // The profile's timescale lines up with the Gantt: the printed span is
    // the chart's.
    app.set_bottom(Some(B::ActivityUsageProfile));
    let printed = app.print_with(&s).unwrap();
    assert!(printed.timescale.is_some());
    // Older stored setups load with the options on.
    let old = serde_json::to_value(&s).unwrap();
    let mut old = old.as_object().unwrap().clone();
    let mut options = old["options"].as_object().unwrap().clone();
    options.remove("profile");
    options.remove("trace_logic");
    old.insert("options".into(), options.into());
    let back: PageSetup = serde_json::from_value(old.into()).unwrap();
    assert!(back.options.profile && back.options.trace_logic);
    // Printing leaves the panes as they were.
    assert!(!app.panes.printing);
    assert_eq!(app.panes.bottom, B::ActivityUsageProfile);
    assert!(app.error.is_none(), "{:?}", app.error);
    let _ = std::fs::remove_file(path);
}

#[test]
fn wbs_projects_resources_and_assignments_views_print_with_their_own_page_setup() {
    use crate::views::MainView;
    let (path, ctx, mut app) = resource_app("views");
    let mut time = 0.;
    let mut run = |app: &mut App| {
        time += 0.1;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
                time: Some(time),
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
    };
    // The Activities view's setup is not the other views'.
    let activities = PageSetup {
        orientation: Orientation::Portrait,
        ..PageSetup::default()
    };
    app.save_page_setup(&activities).unwrap();
    for view in [
        MainView::Wbs,
        MainView::Projects,
        MainView::Resources,
        MainView::Assignments,
    ] {
        app.show_view(view);
        run(&mut app);
        run(&mut app);
        assert!(app.can_print(), "{view:?}");
        let s = app.page_setup();
        assert_eq!(
            s.orientation,
            Orientation::Automatic,
            "{view:?} has its own setup"
        );
        let (pages, text) = print_checked(&mut app, &s);
        assert!(pages >= 1);
        match view {
            MainView::Wbs => {
                for w in &app.project.wbs {
                    assert!(text.contains(w.code.as_str()), "WBS {} printed", w.code);
                }
                let printed = app.print_with(&s).unwrap();
                let bars = printed.pages[0]
                    .ops
                    .iter()
                    .filter(|c| {
                        matches!(&c.op, Op::Fill { color, .. }
                            if *color == crate::views::chart::WBS_BAR)
                    })
                    .count();
                assert!(bars >= app.project.wbs.len(), "WBS bars printed: {bars}");
                // Start date labels left of the first bars are not cut at
                // the chart's left edge: no text starts left of its clip.
                for c in &printed.pages[0].ops {
                    if let Op::Text { pos, text, .. } = &c.op {
                        assert!(pos.x >= c.clip.left() - 0.5, "{text:?} cut at the left");
                    }
                }
            }
            MainView::Resources => assert!(text.contains("Pipefitter")),
            MainView::Assignments => assert!(text.contains("Pipefitter"), "{text}"),
            _ => assert!(text.contains(app.project.model.project.name.as_str())),
        }
        // Without the chart only the table prints, and the setup is kept
        // per view.
        let mut table = s.clone();
        table.options.gantt_chart = false;
        table.orientation = Orientation::Portrait;
        app.save_page_setup(&table).unwrap();
        assert_eq!(app.page_setup(), table);
        let (narrow, _) = print_checked(&mut app, &table);
        assert!(narrow >= 1);
        // Selection and details come back after printing.
        assert!(app.error.is_none(), "{view:?}: {:?}", app.error);
    }
    app.show_view(MainView::Activities);
    run(&mut app);
    assert_eq!(app.page_setup().orientation, Orientation::Portrait);
    assert!(app.page_setup().options.gantt_chart, "the Activities setup");
    app.show_view(MainView::Wbs);
    assert!(!app.page_setup().options.gantt_chart, "the WBS setup");
    let _ = std::fs::remove_file(path);
}

/// Printed timescale labels are chosen at the printed scale: every text run
/// on a baseline stays clear of its neighbours, and hour zoom prints hours.
#[test]
fn printed_timescale_labels_fit_at_hour_and_week_zoom() {
    let (path, _ctx, mut app) = app("print-labels");
    for (ppd, units) in [
        (24. * 4., (timescale::Unit::Day, timescale::Unit::Hour)),
        (24. * 14., (timescale::Unit::Day, timescale::Unit::Hour)),
        (14.5, (timescale::Unit::Week, timescale::Unit::Day)),
        (40., (timescale::Unit::Week, timescale::Unit::Day)),
    ] {
        app.pixels_per_day = ppd as f32;
        (app.view.upper, app.view.lower) = units;
        app.view.automatic_units = false;
        let mut s = PageSetup::default();
        s.options.timescale_start = "DD".into();
        s.options.timescale_finish = "DD+3d".into();
        let printed = app.print_with(&s).unwrap();
        let mut runs: Vec<(f32, f32, f32, &str)> = vec![];
        for page in &printed.pages {
            for c in &page.ops {
                if let Op::Text {
                    pos, text, width, ..
                } = &c.op
                {
                    runs.push((pos.y.round(), pos.x, pos.x + width, text.as_str()));
                }
            }
        }
        runs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        for pair in runs.windows(2) {
            if pair[0].0 == pair[1].0 {
                assert!(
                    pair[0].2 <= pair[1].1 + 0.01,
                    "{ppd}: {:?} overlaps {:?}",
                    pair[0].3,
                    pair[1].3
                );
            }
        }
        let texts: Vec<&str> = runs.iter().map(|r| r.3).collect();
        if units.1 == timescale::Unit::Hour {
            assert!(
                texts.iter().any(|t| t.len() == 2 && t.ends_with('0')),
                "{ppd}: {texts:?}"
            );
        } else if ppd > 30. {
            assert!(
                texts.iter().any(|t| t.starts_with("Mon ")),
                "{ppd}: {texts:?}"
            );
        }
    }
    let _ = std::fs::remove_file(path);
}

/// The footer legend never prints a bar name cut off by its section: in
/// two or more columns each name lies wholly inside the section (a name
/// that cannot fit even in the smallest legend font is shortened with
/// "..." instead of clipped), and a section wide enough shows every name
/// whole. Pages printed with the footer divided into one to five sections.
#[test]
fn footer_legend_names_print_whole_inside_their_section() {
    let (path, _ctx, mut app) = app("legend");
    let names: Vec<String> = app
        .gantt
        .bars
        .iter()
        .filter(|b| b.display)
        .map(|b| b.name.clone())
        .collect();
    let is_entry = |text: &str| {
        names.iter().any(|n| {
            n == text
                || text
                    .strip_suffix("...")
                    .is_some_and(|cut| !cut.is_empty() && n.starts_with(cut))
        })
    };
    for sections in 1..=setup::MAX_SECTIONS {
        let mut s = PageSetup::default();
        s.footer.divide(sections);
        s.footer.sections[0] = setup::Section::with(Content::Legend);
        s.footer.include_on = IncludeOn::AllPages;
        let printed = app.print_with(&s).unwrap();
        let page = &printed.pages[0];
        let mut entries = vec![];
        for c in &page.ops {
            if let Op::Text {
                pos, text, width, ..
            } = &c.op
            {
                if is_entry(text) {
                    assert!(
                        pos.x >= c.clip.left() - 0.5 && pos.x + width <= c.clip.right() + 0.5,
                        "{sections} sections: legend entry {text:?} at {:.1}..{:.1} is cut by its section {:?}",
                        pos.x,
                        pos.x + width,
                        c.clip
                    );
                    entries.push(text.clone());
                }
            }
        }
        assert!(
            !entries.is_empty(),
            "{sections} sections: the legend prints"
        );
        if sections == 1 {
            assert!(
                entries.iter().all(|t| names.contains(t)),
                "a full-width legend shows every name whole: {entries:?}"
            );
        }
    }
    let _ = std::fs::remove_file(path);
}

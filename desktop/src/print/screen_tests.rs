//! Print what is on the screen: pages that never sit side by side unless
//! asked, the Gantt's span and zoom copied from the chart pane, the result
//! line, automatic orientation and band headers kept with their rows.
#![allow(clippy::field_reassign_with_default)]
use super::compose::{scale_line, Summary};
use super::ops::{Op, Page};
use super::paginate::{self, Area, Extent, Flow, RowSlot};
use super::setup::{Content, IncludeOn, Orientation, PageSetup, Scaling};
use super::tests::app;
use super::*;
use crate::grid::layout::GroupLevel;
use crate::grid::Row;
use crate::qa_sweep::Driver;

const LETTER_LANDSCAPE: Area = Area {
    width: 10. * 96.,
    height: 7.5 * 96.,
};

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn wide_extent() -> Extent {
    Extent {
        table_width: 600.,
        span_days: 300.,
        header_height: 35.,
        body_height: 900.,
    }
}

fn narrow_extent() -> Extent {
    Extent {
        table_width: 400.,
        span_days: 20.,
        header_height: 35.,
        body_height: 900.,
    }
}

const ONE_WIDE: Flow = Flow {
    one_wide: true,
    extend: false,
    min_width: 0.,
};

// ------------------------------------------------------------ pagination

#[test]
fn one_wide_holds_every_scaling_choice_to_one_page_across() {
    let wide = wide_extent();
    let width = 600. + 300. * 8.;
    // Adjust is a ceiling: 100% would need 3 pages across.
    let plan = paginate::plan_with(
        Scaling::Adjust { percent: 100 },
        LETTER_LANDSCAPE,
        wide,
        8.,
        ONE_WIDE,
    );
    assert!((plan.scale - 960. / width).abs() < 1e-6);
    assert_eq!(paginate::columns(width, plan.scale, LETTER_LANDSCAPE), 1);
    // A smaller percent than the ceiling is kept.
    let plan = paginate::plan_with(
        Scaling::Adjust { percent: 20 },
        LETTER_LANDSCAPE,
        wide,
        8.,
        ONE_WIDE,
    );
    assert_eq!(plan.scale, 0.2);
    // Fit to 3 wide and Fit timescale 3 wide count as 1 wide.
    let fit = paginate::plan_with(
        Scaling::Fit { wide: 3, tall: 0 },
        LETTER_LANDSCAPE,
        wide,
        8.,
        ONE_WIDE,
    );
    assert!((fit.scale - 960. / width).abs() < 1e-6);
    let timescale = paginate::plan_with(
        Scaling::FitTimescale { wide: 3 },
        LETTER_LANDSCAPE,
        wide,
        8.,
        ONE_WIDE,
    );
    assert!((600. + 300. * timescale.pixels_per_day - 960.).abs() < 1e-2);
    // Without the flow the old math is unchanged.
    let old = paginate::plan(
        Scaling::Fit { wide: 3, tall: 0 },
        LETTER_LANDSCAPE,
        wide,
        8.,
    );
    assert!((old.scale - 2880. / width).abs() < 1e-6);
    // Other content that must fit across counts: a 4000 px bottom pane.
    let plan = paginate::plan_with(
        Scaling::Fit { wide: 1, tall: 0 },
        LETTER_LANDSCAPE,
        narrow_extent(),
        8.,
        Flow {
            min_width: 4000.,
            ..ONE_WIDE
        },
    );
    assert!((plan.scale - 960. / 4000.).abs() < 1e-6);
}

#[test]
fn a_narrow_timescale_is_extended_at_the_screen_zoom_instead_of_stretched() {
    let narrow = narrow_extent();
    let extend = Flow {
        extend: true,
        ..ONE_WIDE
    };
    let plan = paginate::plan_with(
        Scaling::Fit { wide: 1, tall: 0 },
        LETTER_LANDSCAPE,
        narrow,
        8.,
        extend,
    );
    assert_eq!(plan.scale, 1.);
    assert_eq!(plan.pixels_per_day, 8., "the zoom stays the screen's");
    assert!(plan.extended);
    // 400 + 70 * 8 = 960: the chart fills the page.
    assert!((400. + plan.span_days * 8. - 960.).abs() < 8.);
    assert!(plan.span_days > 20.);
    // Without it the zoom stretches, as before.
    let stretched = paginate::plan_with(
        Scaling::Fit { wide: 1, tall: 0 },
        LETTER_LANDSCAPE,
        narrow,
        8.,
        ONE_WIDE,
    );
    assert!(stretched.pixels_per_day > 8.);
    assert!(!stretched.extended);
    assert_eq!(stretched.span_days, 20.);
    // A content that fills the page already is not extended.
    let full = paginate::plan_with(
        Scaling::Fit { wide: 1, tall: 0 },
        LETTER_LANDSCAPE,
        wide_extent(),
        8.,
        extend,
    );
    assert!(!full.extended);
    assert_eq!(full.span_days, 300.);
    // A page limited by its height leaves the width short: extended too.
    let tall = paginate::plan_with(
        Scaling::Fit { wide: 1, tall: 1 },
        LETTER_LANDSCAPE,
        Extent {
            body_height: 4000.,
            ..narrow
        },
        8.,
        extend,
    );
    assert!(tall.scale < 0.2);
    assert!(tall.extended);
    assert!((400. + tall.span_days * 8. - f64::from(960. / tall.scale)).abs() < 8.);
    // Adjust to a percent fills the page the same way.
    let adjust = paginate::plan_with(
        Scaling::Adjust { percent: 100 },
        LETTER_LANDSCAPE,
        narrow,
        8.,
        extend,
    );
    assert_eq!(adjust.scale, 1.);
    assert!(adjust.extended);
}

#[test]
fn blocks_keep_a_band_with_its_first_row() {
    // Rows of 18 px, 4 to a 74 px page; rows 3 and 7 are bands.
    let rows: Vec<RowSlot> = (0..10)
        .map(|k| RowSlot {
            top: k as f32 * 18.,
            height: 18.,
            keep_with_next: k == 3 || k == 7,
            ..RowSlot::default()
        })
        .collect();
    let kept = paginate::blocks(&rows, 180., 74.);
    assert_eq!(kept, vec![(0., 54.), (54., 126.), (126., 180.)]);
    // Without the keep a page would end on the band.
    let loose: Vec<RowSlot> = rows
        .iter()
        .map(|r| RowSlot {
            keep_with_next: false,
            ..*r
        })
        .collect();
    assert_eq!(
        paginate::blocks(&loose, 180., 74.),
        vec![(0., 72.), (72., 144.), (144., 180.)]
    );
    // A chain that is a whole page cannot move: rows still break.
    let chain: Vec<RowSlot> = (0..6)
        .map(|k| RowSlot {
            top: k as f32 * 18.,
            height: 18.,
            keep_with_next: true,
            ..RowSlot::default()
        })
        .collect();
    assert_eq!(
        paginate::blocks(&chain, 108., 54.),
        vec![(0., 54.), (54., 108.)]
    );
}

#[test]
fn blocks_reserve_the_repeated_band_and_give_back_the_room_of_absent_bands() {
    // A page that continues a group repeats its 18 px band row.
    let rows: Vec<RowSlot> = (0..9)
        .map(|k| RowSlot {
            top: k as f32 * 18.,
            height: 18.,
            repeat: (k > 0).then_some((0., 18.)),
            ..RowSlot::default()
        })
        .collect();
    let blocks = paginate::blocks_with(&rows, 162., &|_, _| 72.);
    // The first page holds 4 rows; the next hold the band and 3 rows.
    assert_eq!((blocks[0].y0, blocks[0].y1), (0., 72.));
    assert_eq!(blocks[0].repeat, None);
    assert_eq!((blocks[1].y0, blocks[1].y1), (72., 126.));
    assert_eq!(blocks[1].repeat, Some((0., 18.)));
    // A band that prints on the last page only leaves that page less room.
    let flat: Vec<RowSlot> = (0..6).map(|k| RowSlot::new(k as f32 * 18., 18.)).collect();
    let blocks = paginate::blocks_with(&flat, 108., &|_, last| if last { 90. } else { 126. });
    assert_eq!(blocks.len(), 2, "the last page cannot hold all six rows");
    assert!(blocks[1].y1 - blocks[1].y0 <= 90.);
    let blocks = paginate::blocks_with(&flat, 108., &|_, _| 126.);
    assert_eq!(blocks.len(), 1);
    // Ten rows, 4 rows to a page and 3 on the last.
    let ten: Vec<RowSlot> = (0..10).map(|k| RowSlot::new(k as f32 * 18., 18.)).collect();
    let blocks = paginate::blocks_with(&ten, 180., &|_, last| if last { 54.5 } else { 72.5 });
    let spans: Vec<(f32, f32)> = blocks.iter().map(|b| (b.y0, b.y1)).collect();
    assert_eq!(spans, vec![(0., 72.), (72., 144.), (144., 180.)]);
}

// ------------------------------------------------------------ the result line

#[test]
fn the_result_line_names_pages_scale_and_text_size() {
    assert_eq!(
        scale_line(2, 1, 0.63),
        "Prints on 2 pages, 1 across, at 63% of normal size (about 5 pt text)"
    );
    assert_eq!(
        scale_line(1, 1, 1.),
        "Prints on 1 page at 100% of normal size (about 8 pt text)"
    );
    let summary = |scale: f32, across: usize| Summary {
        pages: 4,
        across,
        scale,
        orientation: Orientation::Landscape,
        paper: "Letter".into(),
        timescale: None,
    };
    assert!(!summary(0.6, 1).small(), "60% is the floor");
    assert!(summary(0.59, 1).small());
    assert!(summary(0.44, 1).small());
    assert_eq!(summary(1., 1).tiled_line(), None);
    assert_eq!(
        summary(1., 2).tiled_line().as_deref(),
        Some("2 pages across: tape or place side by side")
    );
}

#[test]
fn the_summary_matches_the_printout_and_paints_nothing() {
    let (path, _ctx, mut app) = app("summary");
    let mut setups = vec![PageSetup::default()];
    for (scaling, tiling) in [
        (Scaling::Adjust { percent: 100 }, true),
        (Scaling::Adjust { percent: 100 }, false),
        (Scaling::Fit { wide: 2, tall: 1 }, true),
        (Scaling::FitTimescale { wide: 3 }, true),
    ] {
        let mut s = PageSetup::default();
        s.scaling = scaling;
        s.allow_tiling = tiling;
        setups.push(s);
    }
    let mut portrait = PageSetup::default();
    portrait.orientation = Orientation::Portrait;
    portrait.margins.top = 2.;
    portrait.margins.bottom = 2.;
    setups.push(portrait);
    let mut bands = PageSetup::default();
    bands.header.include_on = IncludeOn::FirstPage;
    bands.footer.include_on = IncludeOn::LastPage;
    bands.margins.top = 2.5;
    bands.margins.bottom = 2.5;
    setups.push(bands);
    for (k, s) in setups.iter().enumerate() {
        let before = app.print.paints;
        let summary = app.plan_summary(s).expect("Activities view").unwrap();
        assert_eq!(app.print.paints, before, "setup {k}: nothing is painted");
        let printed = app.print_with(s).unwrap();
        assert_eq!(summary.pages, printed.pages.len(), "setup {k}: pages");
        assert_eq!(summary.across, printed.across, "setup {k}: across");
        assert_eq!(summary.scale, printed.scale, "setup {k}: scale");
        assert_eq!(summary.orientation, printed.orientation, "setup {k}");
        assert_ne!(summary.orientation, Orientation::Automatic);
    }
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ flow

#[test]
fn pages_run_down_only_unless_side_by_side_is_allowed() {
    let (path, _ctx, mut app) = app("down-only");
    for scaling in [
        Scaling::Adjust { percent: 100 },
        Scaling::Adjust { percent: 400 },
        Scaling::Fit { wide: 3, tall: 0 },
        Scaling::FitTimescale { wide: 3 },
    ] {
        let mut s = PageSetup::default();
        s.scaling = scaling;
        s.orientation = Orientation::Landscape;
        s.options.same_as_screen = false;
        assert!(!s.allow_tiling, "the default");
        let printed = app.print_with(&s).unwrap();
        assert_eq!(printed.across, 1, "{scaling:?} is one page across");
        s.allow_tiling = true;
        let tiled = app.print_with(&s).unwrap();
        match scaling {
            Scaling::Adjust { .. } => assert!(tiled.across > 1, "{scaling:?} tiles when allowed"),
            Scaling::Fit { wide, .. } => assert_eq!(tiled.across, wide as usize),
            Scaling::FitTimescale { wide } => assert_eq!(tiled.across, wide as usize),
        }
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn old_setups_load_as_they_were_and_new_ones_start_on_screen_and_down_only() {
    let mut value = serde_json::to_value(PageSetup::default()).unwrap();
    value.as_object_mut().unwrap().remove("allow_tiling");
    let options = value["options"].as_object_mut().unwrap();
    options.remove("same_as_screen");
    options.remove("repeat_band");
    value["orientation"] = serde_json::json!("Portrait");
    let old: PageSetup = serde_json::from_value(value).unwrap();
    assert!(old.allow_tiling, "an old setup keeps tiling");
    assert!(!old.options.same_as_screen, "and its timescale expressions");
    assert!(!old.options.repeat_band);
    assert_eq!(old.orientation, Orientation::Portrait);
    let new = PageSetup::default();
    assert!(!new.allow_tiling && new.options.same_as_screen);
    assert_eq!(new.orientation, Orientation::Automatic);
    // The scaling holds at one page wide until tiling is allowed.
    let mut s = PageSetup {
        scaling: Scaling::Fit { wide: 3, tall: 2 },
        ..PageSetup::default()
    };
    assert_eq!(s.effective_scaling(), Scaling::Fit { wide: 1, tall: 2 });
    s.allow_tiling = true;
    assert_eq!(s.effective_scaling(), Scaling::Fit { wide: 3, tall: 2 });
}

// ------------------------------------------------------------ same as screen

fn screen_driver(name: &str) -> Driver {
    let mut d = Driver::new(name, starter());
    d.frames(3);
    assert!(d.app.print.screen_seen, "the chart was drawn");
    d
}

#[test]
fn same_as_screen_prints_the_visible_span_at_the_screen_zoom() {
    let mut d = screen_driver("same-span");
    // Zoom in on a few weeks, scrolled in.
    d.app.pixels_per_day = 30.;
    d.app.timeline_origin += 12 * DAY + 5 * 60;
    d.frames(2);
    let (origin, ppd, width) = (
        d.app.timeline_origin,
        d.app.pixels_per_day,
        d.app.chart_width,
    );
    // The table alone is narrower than the page so nothing shrinks.
    let mut s = PageSetup::default();
    s.paper = "Tabloid".into();
    s.options.all_columns = false;
    let printed = d.app.print_with(&s).unwrap();
    let (start, finish) = printed.timescale.unwrap();
    assert_eq!(start, origin, "starts at the chart's left edge");
    assert_eq!(printed.pixels_per_day, ppd, "at the screen's zoom");
    assert_eq!(printed.scale, 1.);
    assert_eq!(printed.across, 1);
    let shown = (width / ppd * DAY as f32) as i64;
    assert!(finish - start >= shown, "at least the visible span");
    // A narrower span than the page is extended to the page's width.
    let table = d.app.screen_table_width().round();
    let page: f32 = 16. * 96.;
    let chart = (finish - start) as f64 / DAY as f64 * f64::from(ppd);
    assert!(
        (f64::from(table) + chart - f64::from(page)).abs() < f64::from(ppd) + 1.5,
        "the chart fills the page: {table} + {chart} against {page}"
    );
    // Not the data date range the Options tab would give.
    let expressions = {
        let mut t = s.clone();
        t.options.same_as_screen = false;
        d.app.print_with(&t).unwrap()
    };
    assert_ne!(expressions.timescale.unwrap().0, start);
    // Off: the old stretch of the zoom to fill the page.
    assert!(expressions.pixels_per_day != ppd || expressions.scale != 1.);
}

#[test]
fn same_as_screen_on_a_page_the_span_already_fills_keeps_the_span() {
    let mut d = screen_driver("same-fill");
    let (origin, ppd, width) = (
        d.app.timeline_origin,
        d.app.pixels_per_day,
        d.app.chart_width,
    );
    // All columns and a small page: the content is wider than the page.
    let mut s = PageSetup::default();
    s.paper = "Letter".into();
    s.orientation = Orientation::Landscape;
    let printed = d.app.print_with(&s).unwrap();
    let (start, finish) = printed.timescale.unwrap();
    assert_eq!(start, origin);
    assert_eq!(printed.pixels_per_day, ppd);
    assert!(printed.scale < 1.);
    let span = (finish - start) as f64 / DAY as f64;
    assert!((span * f64::from(ppd) - f64::from(width)).abs() < 2.);
    assert_eq!(printed.across, 1);
}

#[test]
fn same_as_screen_falls_back_to_the_expressions_without_a_screen() {
    let (path, _ctx, mut app) = app("no-screen");
    assert!(!app.print.screen_seen);
    let s = PageSetup::default();
    assert!(s.options.same_as_screen);
    let with = app.print_with(&s).unwrap();
    let mut t = s.clone();
    t.options.same_as_screen = false;
    let without = app.print_with(&t).unwrap();
    assert_eq!(with.timescale, without.timescale);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ orientation

#[test]
fn automatic_orientation_picks_the_larger_scale_then_fewer_pages() {
    let (path, _ctx, mut app) = app("auto");
    let base = PageSetup {
        orientation: Orientation::Automatic,
        ..PageSetup::default()
    };
    let mut cases = vec![base.clone()];
    cases.push(PageSetup {
        scaling: Scaling::Fit { wide: 1, tall: 1 },
        ..base.clone()
    });
    cases.push(PageSetup {
        scaling: Scaling::Adjust { percent: 100 },
        ..base.clone()
    });
    let mut table = base.clone();
    table.options.gantt_chart = false;
    table.options.all_columns = false;
    cases.push(table);
    for (k, s) in cases.iter().enumerate() {
        let mut land = s.clone();
        land.orientation = Orientation::Landscape;
        let mut port = s.clone();
        port.orientation = Orientation::Portrait;
        let l = app.plan_summary(&land).unwrap().unwrap();
        let p = app.plan_summary(&port).unwrap().unwrap();
        let a = app.plan_summary(s).unwrap().unwrap();
        let better = if p.scale > l.scale + 1e-4
            || ((p.scale - l.scale).abs() <= 1e-4 && p.pages < l.pages)
        {
            Orientation::Portrait
        } else {
            Orientation::Landscape
        };
        assert_eq!(a.orientation, better, "case {k}: {l:?} against {p:?}");
        let best = if better == Orientation::Portrait {
            &p
        } else {
            &l
        };
        assert_eq!((a.pages, a.scale), (best.pages, best.scale));
    }
    // A table that needs only 6 inches prints larger on portrait pages (more
    // rows to a page) at the same text size.
    let mut narrow = base;
    narrow.options.gantt_chart = false;
    narrow.options.all_columns = false;
    narrow.scaling = Scaling::Adjust { percent: 40 };
    let a = app.plan_summary(&narrow).unwrap().unwrap();
    assert_eq!(a.scale, 0.4);
    let mut land = narrow.clone();
    land.orientation = Orientation::Landscape;
    let mut port = narrow.clone();
    port.orientation = Orientation::Portrait;
    let (l, p) = (
        app.plan_summary(&land).unwrap().unwrap(),
        app.plan_summary(&port).unwrap().unwrap(),
    );
    if p.pages < l.pages {
        assert_eq!(a.orientation, Orientation::Portrait);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn print_reports_pages_paper_orientation_and_scale() {
    let printed = Printed {
        pages: vec![],
        scale: 0.78,
        orientation: Orientation::Landscape,
        paper: "Tabloid".into(),
        ..Printed::default()
    };
    let text = sent_status(&printed, Path::new("x.pdf"));
    assert!(
        text.starts_with("0 page(s), Tabloid landscape at 78%: sent to the PDF viewer"),
        "{text}"
    );
    // The last paper chosen anywhere starts a layout with no setup.
    let (path, _ctx, mut app) = app("last-paper");
    assert_eq!(app.page_setup().paper, "Letter");
    let a3 = PageSetup {
        paper: "A3".into(),
        ..PageSetup::default()
    };
    app.save_page_setup(&a3).unwrap();
    app.grid.layout.name = "Some other layout".into();
    assert_eq!(app.page_setup().paper, "A3");
    assert_eq!(app.page_setup().orientation, Orientation::Automatic);
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_scale_field_prints_the_page_scale() {
    let (path, _ctx, mut app) = app("scale-field");
    let mut s = PageSetup::default();
    s.footer.sections[1] = setup::Section::text("Scale [scale]", setup::TextAlign::Center);
    let printed = app.print_with(&s).unwrap();
    let text = printed.pages[0].texts().join("\n");
    let expected = format!("Scale {:.0}%", printed.scale * 100.);
    assert!(text.contains(&expected), "{expected} in {text}");
    assert!(setup::FIELDS.iter().any(|(token, _)| *token == "scale"));
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ bands

fn grouped_driver(name: &str) -> Driver {
    let mut d = Driver::new(name, starter());
    d.app.grid.layout.group_sort.levels = vec![GroupLevel {
        column: "wbs".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }];
    d.app.drop_frame_cache();
    d.frames(3);
    d
}

/// Margins that leave a short page, so a printout has many page breaks.
fn short_pages(margin: f32) -> PageSetup {
    let mut s = PageSetup::default();
    s.orientation = Orientation::Landscape;
    s.margins.top = margin;
    s.margins.bottom = margin;
    s.header.height = 0.9;
    s.footer.height = 0.9;
    s
}

#[test]
fn a_page_never_ends_on_a_band_header() {
    let mut d = grouped_driver("keep");
    let rows = d.app.rows();
    let bands = rows.iter().filter(|r| matches!(r, Row::Group(_))).count();
    assert!(bands >= 2, "the starter groups into bands: {bands}");
    let metrics = d.app.row_metrics(&rows);
    let ctx = d.app.print_context();
    let mut breaks = 0;
    for step in 0..40 {
        let margin = 2.0 + step as f32 * 0.02;
        let s = short_pages(margin);
        let job = d.app.compose_job(&ctx, &s, true).unwrap().unwrap();
        for t in job.tiles() {
            if t.y1 >= metrics.offsets.last().copied().unwrap_or(0.) - 0.5 {
                continue;
            }
            breaks += 1;
            // The row that ends this page.
            let last = (0..rows.len())
                .rfind(|k| metrics.offsets[*k] < t.y1 - 0.5)
                .unwrap();
            assert!(
                !matches!(rows[last], Row::Group(_)),
                "margin {margin}: a page ends on the band at row {last}"
            );
        }
    }
    assert!(breaks > 40, "the sweep crossed many page breaks: {breaks}");
}

#[test]
fn a_continued_group_repeats_its_band_row_when_asked() {
    let mut d = grouped_driver("repeat");
    let ctx = d.app.print_context();
    let mut s = short_pages(2.6);
    s.options.same_as_screen = false;
    let plain = d.app.compose_job(&ctx, &s, true).unwrap().unwrap();
    assert!(plain.tiles().iter().all(|t| t.repeat.is_none()));
    s.options.repeat_band = true;
    let job = d.app.compose_job(&ctx, &s, true).unwrap().unwrap();
    let repeated = job.tiles().iter().filter(|t| t.repeat.is_some()).count();
    assert!(repeated > 0, "some page continues a group");
    // The repeated row is a band row: its text is on the page.
    let rows = d.app.rows();
    let labels: Vec<String> = rows
        .iter()
        .filter_map(|r| match r {
            Row::Group(b) => Some(d.app.grid.bands.borrow()[*b].label.clone()),
            _ => None,
        })
        .collect();
    let pages = d.app.printed_pages(&ctx, &job).pages;
    let with_band = |pages: &[Page]| {
        pages
            .iter()
            .filter(|p| {
                let text = p.texts().join("\n");
                labels.iter().any(|l| text.contains(l.as_str()))
            })
            .count()
    };
    let plain_pages = d.app.printed_pages(&ctx, &plain).pages;
    // Lazy pages equal the one-pass construction with repeated rows too.
    let reference = d.app.reference_pages(&ctx, &job);
    for (k, page) in pages.iter().enumerate() {
        assert_eq!(*page, reference[k], "page {k}");
    }
    assert!(with_band(&pages) >= with_band(&plain_pages));
    for (k, t) in job.tiles().iter().enumerate() {
        if let Some((top, _)) = t.repeat {
            let band = rows
                .iter()
                .enumerate()
                .find(|(i, r)| {
                    matches!(r, Row::Group(_)) && d.app.row_metrics(&rows).offsets[*i] == top
                })
                .map(|(i, _)| i)
                .expect("the repeated row is a band");
            let Row::Group(b) = rows[band] else {
                unreachable!()
            };
            let label = d.app.grid.bands.borrow()[b].label.clone();
            let text = pages[k].texts().join("\n");
            assert!(text.contains(&label), "page {k} repeats {label:?}");
        }
    }
}

#[test]
fn bands_on_some_pages_leave_the_room_to_the_others() {
    let mut d = grouped_driver("room");
    let ctx = d.app.print_context();
    let mut s = short_pages(2.4);
    s.options.same_as_screen = false;
    let all = d.app.compose_job(&ctx, &s, true).unwrap().unwrap();
    s.header.include_on = IncludeOn::FirstPage;
    s.footer.include_on = IncludeOn::LastPage;
    let some = d.app.compose_job(&ctx, &s, true).unwrap().unwrap();
    assert!(
        some.count() <= all.count(),
        "bands that print less never need more pages: {} against {}",
        some.count(),
        all.count()
    );
    let pages = d.app.printed_pages(&ctx, &some).pages;
    // A middle page rises to the top margin, where the header would be.
    if pages.len() > 2 {
        let top = |p: &Page| {
            p.ops
                .iter()
                .filter_map(|c| match &c.op {
                    Op::Stroke { points, .. } => Some(points[0].y),
                    _ => None,
                })
                .fold(f32::INFINITY, f32::min)
        };
        assert!(
            top(&pages[1]) < top(&pages[0]),
            "{} {}",
            top(&pages[1]),
            top(&pages[0])
        );
    }
    // Header and footer text are on the pages they were asked for.
    let first = pages[0].texts().join("\n");
    let last = pages[pages.len() - 1].texts().join("\n");
    assert!(first.contains("Data Date"));
    assert!(last.contains(&format!("Page {} of {}", pages.len(), pages.len())));
    assert!(pages
        .iter()
        .skip(1)
        .all(|p| !p.texts().join("\n").contains("Data Date")));
    // Lazy pages equal the one-pass construction here too.
    let reference = d.app.reference_pages(&ctx, &some);
    for (k, page) in pages.iter().enumerate() {
        assert_eq!(*page, reference[k], "page {k}");
    }
    let _ = Content::None;
}

// ------------------------------------------------------------ dialog

#[test]
fn page_setup_shows_the_result_line_and_the_new_choices() {
    let mut d = screen_driver("dialog");
    d.app.open_page_setup();
    d.frames(3);
    for action in [
        "page_setup_automatic",
        "page_setup_tiling",
        "page_setup_portrait",
        "page_setup_landscape",
    ] {
        assert!(d.has(action), "{action}");
    }
    let dialog = d.app.print.dialog.as_ref().unwrap();
    let (_, summary) = dialog
        .summary
        .as_ref()
        .expect("the result line is worked out");
    assert!(summary.is_ok(), "{summary:?}");
    assert_eq!(dialog.draft.orientation, Orientation::Automatic);
    // Fit and Fit timescale are held at one page wide.
    d.click("page_setup_scaling_1");
    d.frames(2);
    d.click("page_setup_tiling");
    d.frames(2);
    assert!(d.app.print.dialog.as_ref().unwrap().draft.allow_tiling);
    d.click("page_setup_tiling");
    d.frames(2);
    assert!(!d.app.print.dialog.as_ref().unwrap().draft.allow_tiling);
    // The summary follows a change at once.
    d.click("page_setup_portrait");
    d.frames(2);
    let dialog = d.app.print.dialog.as_ref().unwrap();
    let (kept, summary) = dialog.summary.as_ref().unwrap();
    assert_eq!(kept.orientation, Orientation::Portrait);
    assert_eq!(summary.as_ref().unwrap().orientation, Orientation::Portrait);
    // Options: Same as screen ticks and unticks; the timescale boxes stay open.
    d.click("page_setup_tab_4");
    d.frames(2);
    assert!(d.has("page_setup_same_as_screen"));
    assert!(
        d.app
            .print
            .dialog
            .as_ref()
            .unwrap()
            .draft
            .options
            .same_as_screen
    );
    d.click("page_setup_same_as_screen");
    d.frames(2);
    assert!(
        !d.app
            .print
            .dialog
            .as_ref()
            .unwrap()
            .draft
            .options
            .same_as_screen
    );
    d.click("page_setup_same_as_screen");
    d.frames(2);
    assert!(
        d.app
            .print
            .dialog
            .as_ref()
            .unwrap()
            .draft
            .options
            .same_as_screen
    );
    // Cancel leaves the saved setup alone.
    let before = d.app.page_setup();
    d.click("page_setup_cancel");
    d.frames(2);
    assert!(d.app.print.dialog.is_none());
    assert_eq!(d.app.page_setup(), before);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn small_text_offers_a_larger_paper_and_landscape() {
    let mut d = screen_driver("remedies");
    // A table of every column on a Letter portrait page prints small.
    let mut s = PageSetup::default();
    s.orientation = Orientation::Portrait;
    s.options.same_as_screen = false;
    s.options.timescale_finish = "PF+2Y".into();
    d.app.save_page_setup(&s).unwrap();
    d.app.open_page_setup();
    d.frames(3);
    let dialog = d.app.print.dialog.as_ref().unwrap();
    let summary = dialog.summary.as_ref().unwrap().1.clone().unwrap();
    assert!(summary.small(), "{summary:?}");
    assert!(d.has("page_setup_larger_paper"));
    assert!(d.has("page_setup_use_landscape"));
    d.click("page_setup_use_landscape");
    d.frames(2);
    assert_eq!(
        d.app.print.dialog.as_ref().unwrap().draft.orientation,
        Orientation::Landscape
    );
    d.click("page_setup_larger_paper");
    d.frames(2);
    assert_eq!(d.app.print.dialog.as_ref().unwrap().draft.paper, "Tabloid");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.dialog.is_none());
    assert_eq!(d.app.page_setup().orientation, Orientation::Portrait);
}

// ------------------------------------------------------------ renders

/// Renders the new Page Setup choices and the preview status for a look:
/// `artifacts/review/print/screen-*.png` (ignored by git).
#[test]
fn render_print_what_is_on_the_screen_for_review() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/print");
    std::fs::create_dir_all(&out).unwrap();
    let (path, ctx, mut app) = app("render-screen");
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let crop = |app: &mut App,
                painter: &mut SoftwarePainter,
                ctx: &egui::Context,
                time: &mut f64,
                file: &str| {
        for _ in 0..2 {
            let _ = painter.frame(app, ctx, time, vec![], false);
        }
        let (image, _) = painter.frame(app, ctx, time, vec![], true).unwrap();
        let close = app.action_rects["page_setup_title_close"];
        let size = dialog::CLIENT + Vec2::new(2., 33.);
        let left = (close[2] + 1. - size.x).round() as u32;
        let top = (close[1] - 1.).round() as u32;
        image::imageops::crop_imm(&image, left, top, size.x as u32, size.y as u32)
            .to_image()
            .save(out.join(file))
            .unwrap();
    };
    painter.click_action(&mut app, &ctx, &mut time, "menu_bar_file");
    painter.click_action(&mut app, &ctx, &mut time, "menu_page_setup");
    assert!(app.print.dialog.is_some());
    crop(
        &mut app,
        &mut painter,
        &ctx,
        &mut time,
        "screen-page-default.png",
    );
    // Portrait on Letter with a long timescale: small text.
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_portrait");
    app.print
        .dialog
        .as_mut()
        .unwrap()
        .draft
        .options
        .same_as_screen = false;
    app.print
        .dialog
        .as_mut()
        .unwrap()
        .draft
        .options
        .timescale_finish = "PF+3Y".into();
    crop(
        &mut app,
        &mut painter,
        &ctx,
        &mut time,
        "screen-page-small.png",
    );
    // Side by side allowed: the warning line.
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_tiling");
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_scaling_0");
    crop(
        &mut app,
        &mut painter,
        &ctx,
        &mut time,
        "screen-page-tiled.png",
    );
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_default");
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_tab_4");
    crop(
        &mut app,
        &mut painter,
        &ctx,
        &mut time,
        "screen-options-same.png",
    );
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_same_as_screen");
    crop(
        &mut app,
        &mut painter,
        &ctx,
        &mut time,
        "screen-options-expressions.png",
    );
    // Preview of a tiled printout shows the warning in its status bar.
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_tab_0");
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_tiling");
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_scaling_0");
    painter.click_action(&mut app, &ctx, &mut time, "page_setup_ok");
    painter.click_action(&mut app, &ctx, &mut time, "tool_print_preview");
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("screen-preview-tiled.png")).unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "preview_close");
    // The default setup on a zoomed-in chart: what is on screen.
    app.pixels_per_day = 28.;
    app.save_page_setup(&app.starting_page_setup()).unwrap();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "tool_print_preview");
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("screen-preview-default.png")).unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "preview_close");
    // Grouped, short pages, the band row repeated on continued pages.
    app.grid.layout.group_sort.levels = vec![GroupLevel {
        column: "wbs".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }];
    app.drop_frame_cache();
    let mut grouped = short_pages(2.4);
    grouped.options.repeat_band = true;
    app.save_page_setup(&grouped).unwrap();
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "tool_print_preview");
    for _ in 0..4 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "preview_right");
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("screen-preview-repeat.png")).unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn trailing_timescale_that_only_pads_the_last_page_is_cut() {
    let (path, _ctx, mut app) = app("tail");
    let mut s = PageSetup {
        scaling: Scaling::Adjust { percent: 100 },
        orientation: Orientation::Landscape,
        allow_tiling: true,
        ..PageSetup::default()
    };
    s.options.same_as_screen = false;
    s.options.timescale_finish = "PF+1W".into();
    let short = app.print_with(&s).unwrap();
    // Two more months of empty timescale: no more pages across than the
    // last bar and its labels need.
    s.options.timescale_finish = "PF+60D".into();
    let long = app.print_with(&s).unwrap();
    assert!(
        long.across <= short.across + 1,
        "{} pages across against {}",
        long.across,
        short.across
    );
    let (_, finish) = long.timescale.unwrap();
    let (_, short_finish) = short.timescale.unwrap();
    assert!(finish < short_finish + 60 * DAY, "the empty tail was cut");
    let _ = std::fs::remove_file(path);
}

/// Fit to N pages tall: rows that do not split evenly over the pages at the
/// scale the height allows would spill onto page N+1, so the scale shrinks
/// until the rows fit on N pages.
#[test]
fn fit_to_pages_tall_never_prints_more_pages() {
    let area = Area {
        width: 960.,
        height: 300.,
    };
    for (tall, n) in [(2, 7), (3, 10)] {
        let rows: Vec<RowSlot> = (0..n)
            .map(|k| RowSlot::new(k as f32 * 100., 100.))
            .collect();
        let body = n as f32 * 100.;
        let plan = paginate::plan_with(
            Scaling::Fit { wide: 1, tall },
            area,
            Extent {
                table_width: 400.,
                span_days: 0.,
                header_height: 0.,
                body_height: body,
            },
            8.,
            Flow::default(),
        );
        let blocks_at =
            |scale: f32| paginate::blocks_with(&rows, body, &|_, _| area.height / scale).len();
        assert!(
            blocks_at(plan.scale) > tall as usize,
            "the defect at {tall}"
        );
        let scale = paginate::fit_tall(tall, plan.scale, &blocks_at);
        assert!(blocks_at(scale) <= tall as usize, "{tall}: {scale}");
        assert!(
            scale > plan.scale * 0.7,
            "{tall}: no more shrink than needed"
        );
    }
}

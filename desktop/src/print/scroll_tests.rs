//! Scrolling in Print Preview and the report preview: the wheel, Ctrl+wheel,
//! Shift+wheel, the keys, a drag on the page, the page buttons, and that
//! only the pages in view are built.
use super::setup::{Orientation, PageSetup, Scaling};
use super::viewer::{self, WHEEL_GAIN};
use super::viewer_ui;
use super::*;
use crate::qa_sweep::Driver;

const POINTER: Pos2 = Pos2::new(700., 400.);

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

/// Landscape pages with tall margins: the 14-activity starter prints on
/// several pages.
fn short_pages() -> PageSetup {
    let mut s = PageSetup {
        orientation: Orientation::Landscape,
        ..PageSetup::default()
    };
    s.margins.top = 3.1;
    s.margins.bottom = 3.1;
    s.header.height = 0.9;
    s.footer.height = 0.9;
    s
}

fn driver(name: &str, setup: &PageSetup) -> Driver {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    let mut d = Driver::new(name, p);
    d.home();
    d.app.save_page_setup(setup).unwrap();
    d.click("tool_print_preview");
    settle(&mut d);
    d
}

fn settle(d: &mut Driver) {
    for _ in 0..40 {
        if d.app.print_preview_ready() {
            d.frames(3);
            return;
        }
        d.frames(1);
    }
    panic!("the preview never finished");
}

fn preview(d: &Driver) -> &preview::Preview {
    d.app.print.preview.as_ref().expect("a preview is open")
}

/// A wheel turn over the pointer; egui spreads it over the frames after.
fn wheel(d: &mut Driver, delta: Vec2, modifiers: egui::Modifiers) {
    let _ = d.frame_with(
        vec![
            egui::Event::PointerMoved(POINTER),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta,
                modifiers,
            },
        ],
        modifiers,
    );
    d.frames(5);
}

fn scroll(d: &Driver) -> Vec2 {
    preview(d).nav.scroll
}

/// A notch of the wheel, in screen pixels of scroll.
const NOTCH: f32 = 40. * WHEEL_GAIN;

#[test]
fn the_wheel_scrolls_through_the_pages_at_every_zoom() {
    let mut d = driver("scroll-wheel", &short_pages());
    let count = preview(&d).count();
    assert!(count >= 4, "{count} pages to scroll through");
    assert_eq!(preview(&d).page, 0);
    for zoom in [None, Some(1.), Some(0.4), Some(2.)] {
        d.app.print.preview.as_mut().unwrap().zoom = zoom;
        d.app.print.preview.as_mut().unwrap().go(0);
        d.frames(3);
        assert_eq!(scroll(&d).y, 0., "at {zoom:?}");
        wheel(&mut d, Vec2::new(0., -1.), Default::default());
        let one = scroll(&d).y;
        let content = one > 0.;
        // At fit a page fills the view: it still scrolls down through the stack.
        assert!(content, "at {zoom:?}: scrolled {one}");
        assert!(
            (one - NOTCH).abs() < 2.,
            "at {zoom:?}: {one} against {NOTCH}"
        );
        // Down through every page to the end, up again to the start.
        let mut seen = vec![preview(&d).page];
        for _ in 0..80 {
            let before = scroll(&d).y;
            wheel(&mut d, Vec2::new(0., -3.), Default::default());
            let page = preview(&d).page;
            if seen.last() != Some(&page) {
                seen.push(page);
            }
            if scroll(&d).y == before {
                break;
            }
        }
        assert_eq!(seen.last(), Some(&(count - 1)), "at {zoom:?}: {seen:?}");
        assert!(
            seen.windows(2).all(|w| w[1] > w[0]),
            "pages count up as the view moves down: {seen:?}"
        );
        for _ in 0..80 {
            let before = scroll(&d).y;
            wheel(&mut d, Vec2::new(0., 3.), Default::default());
            if scroll(&d).y == before {
                break;
            }
        }
        assert_eq!(scroll(&d).y, 0., "at {zoom:?}");
        assert_eq!(preview(&d).page, 0);
    }
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_current_page_follows_the_page_at_the_top_of_the_view() {
    let mut d = driver("scroll-current", &short_pages());
    d.app.print.preview.as_mut().unwrap().zoom = Some(0.8);
    d.frames(3);
    let mut last = 0;
    let mut turns = 0;
    while preview(&d).page + 1 < preview(&d).count() && turns < 120 {
        wheel(&mut d, Vec2::new(0., -1.), Default::default());
        turns += 1;
        // The page whose top is nearest above the view's top is the current one.
        let p = preview(&d);
        let shown = &p.nav.shown;
        let first = shown.iter().map(|(k, _)| *k).min().unwrap();
        assert!(
            p.page == first || p.page == first + 1,
            "page {} with pages {:?} in view",
            p.page,
            shown.iter().map(|(k, _)| *k).collect::<Vec<_>>()
        );
        assert!(p.page >= last, "never goes back while scrolling down");
        last = p.page;
    }
    assert_eq!(preview(&d).page + 1, preview(&d).count());
}

#[test]
fn the_page_buttons_scroll_to_the_page() {
    let mut d = driver("scroll-buttons", &short_pages());
    let count = preview(&d).count();
    d.click("preview_right");
    assert_eq!(preview(&d).page, 1);
    d.frames(3);
    let at = scroll(&d).y;
    assert!(at > 0., "scrolled to page 2");
    assert_eq!(preview(&d).page, 1, "and it stays the current page");
    // The page's top is at the view's top.
    let (k, origin) = preview(&d)
        .nav
        .shown
        .iter()
        .copied()
        .find(|(k, _)| *k == 1)
        .unwrap();
    assert_eq!(k, 1);
    let body_top = d.rect("preview_close").bottom() + 2.;
    assert!(
        (origin.y - body_top).abs() < 30.,
        "page 2 starts at {} against the view top near {}",
        origin.y,
        body_top
    );
    d.click("preview_left");
    assert_eq!(preview(&d).page, 0);
    d.frames(3);
    assert_eq!(scroll(&d).y, 0.);
    d.click("preview_last");
    d.frames(3);
    assert_eq!(preview(&d).page, count - 1);
    assert!(scroll(&d).y > at);
    d.click("preview_first");
    d.frames(3);
    assert_eq!((preview(&d).page, scroll(&d).y), (0, 0.));
    assert!(
        !d.has("preview_down"),
        "rows of pages only where pages run across"
    );
}

#[test]
fn shift_wheel_and_a_sideways_wheel_scroll_across_a_wide_page() {
    let mut d = driver("scroll-sideways", &short_pages());
    // Fit: nothing wider than the view.
    wheel(&mut d, Vec2::new(0., -2.), egui::Modifiers::SHIFT);
    assert_eq!(scroll(&d).x, 0.);
    d.app.print.preview.as_mut().unwrap().zoom = Some(3.);
    d.app.print.preview.as_mut().unwrap().go(0);
    d.frames(3);
    let (x0, y) = (scroll(&d).x, scroll(&d).y);
    wheel(&mut d, Vec2::new(0., -2.), egui::Modifiers::SHIFT);
    let x = scroll(&d).x;
    assert!(
        (x - x0 - 2. * NOTCH).abs() < 2.,
        "shift+wheel went {}",
        x - x0
    );
    assert_eq!(scroll(&d).y, y, "and not down");
    wheel(&mut d, Vec2::new(-1., 0.), Default::default());
    assert!(
        (scroll(&d).x - x0 - 3. * NOTCH).abs() < 2.,
        "a sideways wheel"
    );
    wheel(&mut d, Vec2::new(0., 1.), egui::Modifiers::SHIFT);
    assert!((scroll(&d).x - x0 - 2. * NOTCH).abs() < 2., "and back");
}

/// Page-pixel position under `pos` on the page drawn there, if any.
fn page_point(d: &Driver, pos: Pos2) -> Option<(usize, Vec2)> {
    let p = preview(d);
    let zoom = p.zoom.unwrap();
    let size = p.page_size();
    p.nav.shown.iter().find_map(|(k, o)| {
        let r = Rect::from_min_size(*o, size * zoom);
        r.contains(pos).then(|| (*k, (pos - *o) / zoom))
    })
}

#[test]
fn ctrl_wheel_zooms_about_the_pointer_and_the_zoom_controls_follow() {
    let mut d = driver("scroll-zoom", &short_pages());
    d.app.print.preview.as_mut().unwrap().zoom = Some(0.8);
    d.frames(3);
    wheel(&mut d, Vec2::new(0., -2.), Default::default());
    let before = page_point(&d, POINTER).expect("a page under the pointer");
    let z0 = preview(&d).zoom.unwrap();
    wheel(&mut d, Vec2::new(0., 2.), ctrl());
    let z1 = preview(&d).zoom.unwrap();
    assert!(z1 > z0 * 1.3, "zoomed in from {z0} to {z1}");
    let after = page_point(&d, POINTER).expect("a page under the pointer");
    assert_eq!(after.0, before.0);
    // The page is narrower than the view and stays centered; the height
    // is what the pointer holds.
    let drift = (after.1.y - before.1.y).abs() * z1;
    assert!(
        drift < 3.,
        "the page point under the pointer moved {drift} px"
    );
    wheel(&mut d, Vec2::new(0., -4.), ctrl());
    let z2 = preview(&d).zoom.unwrap();
    assert!(z2 < z1, "ctrl+wheel down zooms out ({z2})");
    // Zoom In and Zoom Out step from where the wheel left the zoom.
    d.click("preview_zoom_in");
    d.frames(2);
    let z3 = preview(&d).zoom.unwrap();
    assert!((z3 / z2 - 1.25).abs() < 1e-3, "{z3} from {z2}");
    d.click("preview_zoom_out");
    d.frames(2);
    assert!((preview(&d).zoom.unwrap() - z2).abs() < 1e-3);
    // The limits hold.
    for _ in 0..40 {
        wheel(&mut d, Vec2::new(0., 5.), ctrl());
    }
    assert!(preview(&d).zoom.unwrap() <= viewer::MAX_ZOOM + 1e-4);
    for _ in 0..40 {
        wheel(&mut d, Vec2::new(0., -5.), ctrl());
    }
    assert!(preview(&d).zoom.unwrap() >= viewer::MIN_ZOOM - 1e-4);
    // Zoom to Fit brings the page back to the view.
    d.click("preview_fit");
    d.frames(3);
    assert!(preview(&d).zoom.is_none());
}

#[test]
fn page_keys_home_end_and_arrows_move_the_view_and_escape_closes() {
    let mut d = driver("scroll-keys", &short_pages());
    let count = preview(&d).count();
    let selected = d.app.selected;
    d.key(Key::PageDown);
    d.frames(2);
    assert_eq!(preview(&d).page, 1);
    d.key(Key::PageDown);
    d.frames(2);
    assert_eq!(preview(&d).page, 2);
    d.key(Key::PageUp);
    d.frames(2);
    assert_eq!(preview(&d).page, 1);
    d.key(Key::End);
    d.frames(2);
    assert_eq!(preview(&d).page, count - 1);
    d.key(Key::Home);
    d.frames(2);
    assert_eq!((preview(&d).page, scroll(&d).y), (0, 0.));
    d.key(Key::ArrowDown);
    assert_eq!(scroll(&d).y, viewer::ARROW_STEP);
    d.key(Key::ArrowDown);
    assert_eq!(scroll(&d).y, 2. * viewer::ARROW_STEP);
    d.key(Key::ArrowUp);
    assert_eq!(scroll(&d).y, viewer::ARROW_STEP);
    d.app.print.preview.as_mut().unwrap().zoom = Some(3.);
    d.frames(3);
    let x = scroll(&d).x;
    d.key(Key::ArrowRight);
    assert_eq!(scroll(&d).x, x + viewer::ARROW_STEP);
    d.key(Key::ArrowLeft);
    assert_eq!(scroll(&d).x, x);
    assert_eq!(d.app.selected, selected, "the table behind took no key");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.preview.is_none());
}

#[test]
fn a_drag_on_the_page_pans_it() {
    let mut d = driver("scroll-drag", &short_pages());
    d.app.print.preview.as_mut().unwrap().zoom = Some(1.);
    d.frames(3);
    let start = POINTER;
    let _ = d.frame(vec![egui::Event::PointerMoved(start)]);
    let button = |pressed| egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![button(true)]);
    for step in 1..=6 {
        let _ = d.frame(vec![egui::Event::PointerMoved(
            start - Vec2::new(10., 10.) * step as f32,
        )]);
    }
    let _ = d.frame(vec![egui::Event::PointerMoved(start - Vec2::new(60., 60.))]);
    let _ = d.frame(vec![egui::Event::PointerButton {
        pos: start - Vec2::new(60., 60.),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    d.frames(2);
    let s = scroll(&d);
    assert!((55. ..=70.).contains(&s.y), "dragged up 60: scrolled {s:?}");
    assert!(d.app.print.preview.is_some(), "a drag is not a click");
}

#[test]
fn only_the_pages_in_view_are_built() {
    let mut d = driver("scroll-lazy", &short_pages());
    let count = preview(&d).count();
    assert!(count >= 4);
    // Fit shows one page: it and nothing else is built.
    let built = preview(&d).built();
    assert_eq!(built, 1, "{built} of {count} pages built when it opens");
    assert!(preview(&d).has_page(0));
    assert!(!preview(&d).has_page(count - 1));
    // Scrolling to the last page builds it; the first is not asked for again.
    d.key(Key::End);
    d.frames(6);
    assert!(preview(&d).has_page(count - 1));
    assert!(
        !preview(&d).has_page(1),
        "a page never in view is not built"
    );
    assert!(
        preview(&d).built() < count,
        "{} of {count} pages built",
        preview(&d).built()
    );
    // Scrolled past, a page stays built for the way back; none are built that
    // were never in view.
    let in_view: Vec<usize> = preview(&d).nav.shown.iter().map(|(k, _)| *k).collect();
    for k in 0..count {
        if preview(&d).has_page(k) {
            assert!(
                k == 0 || in_view.contains(&k),
                "page {k} was built without being in view {in_view:?}"
            );
        }
    }
    // Far out, many pages are in view, and a frame builds only a few of them.
    d.app.print.preview.as_mut().unwrap().zoom = Some(viewer::MIN_ZOOM);
    d.app.print.preview.as_mut().unwrap().go(0);
    let before = preview(&d).built();
    d.frames(1);
    let after_one = preview(&d).built();
    assert!(
        after_one <= before + 2 + 1,
        "{before} then {after_one} pages built in one frame"
    );
    d.frames(12);
    let in_view = preview(&d).nav.shown.len();
    assert!(in_view >= 3, "{in_view} pages in view at the smallest zoom");
    for (k, _) in preview(&d).nav.shown.clone() {
        assert!(preview(&d).has_page(k), "page {k} in view is built");
    }
}

#[test]
fn tiled_pages_run_across_and_scroll_both_ways() {
    let mut s = PageSetup {
        scaling: Scaling::Adjust { percent: 250 },
        allow_tiling: true,
        orientation: Orientation::Landscape,
        ..PageSetup::default()
    };
    s.options.same_as_screen = false;
    let mut d = driver("scroll-tiled", &s);
    let p = preview(&d);
    let (count, across) = (p.count(), p.across());
    assert!(
        across > 1 && count > across,
        "{count} pages, {across} across"
    );
    d.app.print.preview.as_mut().unwrap().zoom = Some(0.5);
    d.frames(3);
    // Page 1 sits right of page 0 on the first row.
    let shown = preview(&d).nav.shown.clone();
    let at = |k: usize| shown.iter().find(|(i, _)| *i == k).map(|(_, o)| *o);
    let (a, b) = (at(0).unwrap(), at(1).unwrap());
    assert_eq!(a.y, b.y);
    assert!(b.x > a.x);
    d.click("preview_down");
    assert_eq!(preview(&d).page, across);
    d.click("preview_right");
    assert_eq!(preview(&d).page, across + 1);
    d.frames(3);
    d.key(Key::Home);
    d.frames(3);
    assert_eq!(preview(&d).page, 0);
}

#[test]
fn scrolling_does_not_touch_the_printout_or_the_chart_behind() {
    let mut d = driver("scroll-print", &short_pages());
    let setup = d.app.page_setup();
    let before = d.app.print_with(&setup).unwrap().pages;
    let (ppd, origin, grid_x) = (
        d.app.pixels_per_day,
        d.app.timeline_origin,
        d.app.grid.scroll_x,
    );
    wheel(&mut d, Vec2::new(0., -5.), Default::default());
    wheel(&mut d, Vec2::new(0., -5.), egui::Modifiers::SHIFT);
    wheel(&mut d, Vec2::new(0., 5.), ctrl());
    d.key(Key::PageDown);
    d.frames(2);
    assert_eq!(
        (
            d.app.pixels_per_day,
            d.app.timeline_origin,
            d.app.grid.scroll_x
        ),
        (ppd, origin, grid_x),
        "the chart and the table behind the preview did not move"
    );
    let after = d.app.print_with(&setup).unwrap().pages;
    assert_eq!(before, after, "the printout is what it was");
    // And the pages the preview built are the printed pages.
    let ctx = d.app.print_context();
    let mut p = d.app.print.preview.take().unwrap();
    let pages = p.all_pages(&d.app, &ctx);
    d.app.print.preview = Some(p);
    assert_eq!(pages.len(), after.len());
    for (k, page) in after.iter().enumerate() {
        assert_eq!(*page, *pages[k], "page {k}");
    }
}

/// A report preview of `n` pages of blank letter paper with a line of text.
fn report_pages(n: usize) -> Vec<ops::Page> {
    (0..n)
        .map(|k| ops::Page {
            size: Vec2::new(816., 1056.),
            ops: vec![ops::Clipped {
                clip: Rect::from_min_size(Pos2::ZERO, Vec2::new(816., 1056.)),
                op: ops::Op::Text {
                    pos: Pos2::new(60., 80.),
                    text: format!("Report page {}", k + 1),
                    size: 14.,
                    bold: false,
                    color: Color32::BLACK,
                    width: 200.,
                },
            }],
        })
        .collect()
}

/// Opens a report preview of `n` pages as Tools > Reports would.
fn open_report(d: &mut Driver, n: usize) {
    d.app.reports.page_preview = Some(crate::reports::rpt_run::PagePreview::from_pages(
        "Report".into(),
        report_pages(n),
        vec![],
        ("<html>report</html>".into(), "report text".into()),
        d.app.viewer_start(),
    ));
}

fn report_preview(d: &Driver) -> &crate::reports::rpt_run::PagePreview {
    d.app
        .reports
        .page_preview
        .as_ref()
        .expect("a report preview")
}

#[test]
fn the_report_preview_scrolls_the_same_way() {
    let p = openxer_xer::starter();
    let mut d = Driver::new("scroll-report", p);
    d.home();
    open_report(&mut d, 6);
    d.frames(4);
    let pointer = d.rect("report_preview_first").center() + Vec2::new(300., 200.);
    let wheel = |d: &mut Driver, delta: Vec2, m: egui::Modifiers| {
        let _ = d.frame_with(
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta,
                    modifiers: m,
                },
            ],
            m,
        );
        d.frames(5);
    };
    assert_eq!(report_preview(&d).nav.scroll, Vec2::ZERO);
    wheel(&mut d, Vec2::new(0., -1.), Default::default());
    assert!(
        (report_preview(&d).nav.scroll.y - NOTCH).abs() < 2.,
        "{:?}",
        report_preview(&d).nav.scroll
    );
    for _ in 0..60 {
        wheel(&mut d, Vec2::new(0., -3.), Default::default());
    }
    assert_eq!(report_preview(&d).page, 5, "scrolled down to the last page");
    wheel(&mut d, Vec2::new(0., 3.), ctrl());
    let z = report_preview(&d).zoom.expect("Ctrl+wheel set a zoom");
    assert!(z > 0.1);
    d.click("report_preview_zoom_out");
    d.frames(2);
    assert!(report_preview(&d).zoom.unwrap() < z);
    d.click("report_preview_first");
    d.frames(3);
    assert_eq!(
        (report_preview(&d).page, report_preview(&d).nav.scroll.y),
        (0, 0.)
    );
    d.click("report_preview_next");
    d.frames(3);
    assert_eq!(report_preview(&d).page, 1);
    assert!(report_preview(&d).nav.scroll.y > 0.);
    d.key(Key::End);
    d.frames(3);
    assert_eq!(report_preview(&d).page, 5);
    d.key(Key::Home);
    d.frames(3);
    assert_eq!(report_preview(&d).page, 0);
    d.key(Key::PageDown);
    d.frames(3);
    assert_eq!(report_preview(&d).page, 1);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.page_preview.is_none());
}

/// Renders the scrolled preview for a look: `artifacts/previewscroll/*.png`
/// (ignored by git).
#[test]
fn render_the_scrolled_previews_for_review() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/previewscroll");
    std::fs::create_dir_all(&out).unwrap();
    let (path, ctx, mut app) = super::tests::app("render-scroll");
    app.save_page_setup(&short_pages()).unwrap();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "tool_print_preview");
    for _ in 0..6 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let mut shot =
        |app: &mut App, painter: &mut SoftwarePainter, events: Vec<egui::Event>, file: &str| {
            let _ = painter.frame(app, &ctx, &mut time, events, false);
            for _ in 0..5 {
                let _ = painter.frame(app, &ctx, &mut time, vec![], false);
            }
            let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
            image.save(out.join(file)).unwrap();
        };
    let wheel = |delta: Vec2, modifiers: egui::Modifiers| {
        vec![
            egui::Event::PointerMoved(POINTER),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta,
                modifiers,
            },
        ]
    };
    shot(&mut app, &mut painter, vec![], "preview-fit.png");
    app.print.preview.as_mut().unwrap().zoom = Some(0.45);
    shot(&mut app, &mut painter, vec![], "preview-stack.png");
    shot(
        &mut app,
        &mut painter,
        wheel(Vec2::new(0., -5.), Default::default()),
        "preview-wheel.png",
    );
    shot(
        &mut app,
        &mut painter,
        wheel(Vec2::new(0., 3.), ctrl()),
        "preview-ctrl-wheel.png",
    );
    // The viewer with the page strip, at Fit Page and Fit Width, and Find.
    {
        let p = app.print.preview.as_mut().unwrap();
        p.zoom = None;
        p.view.thumbs = true;
    }
    shot(&mut app, &mut painter, vec![], "viewer-fit-page-strip.png");
    app.print.preview.as_mut().unwrap().view.fit = viewer::Fit::Width;
    shot(&mut app, &mut painter, vec![], "viewer-fit-width-strip.png");
    {
        let p = app.print.preview.as_mut().unwrap();
        p.view.fit = viewer::Fit::Page;
        p.view.find.open = true;
        p.view.find.query = "construction".into();
    }
    shot(&mut app, &mut painter, vec![], "viewer-find.png");
    app.print.preview = None;
    let (_, view) = app.viewer_start();
    app.reports.page_preview = Some(crate::reports::rpt_run::PagePreview::from_pages(
        "Report".into(),
        report_pages(6),
        vec![],
        ("<html></html>".into(), "text".into()),
        (Some(0.45), view),
    ));
    app.reports.page_preview.as_mut().unwrap().view.thumbs = true;
    shot(&mut app, &mut painter, vec![], "report-stack.png");
    shot(
        &mut app,
        &mut painter,
        wheel(Vec2::new(0., -4.), Default::default()),
        "report-wheel.png",
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ the viewer

fn type_into(d: &mut Driver, action: &str, text: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.frames(1);
}

fn view(d: &Driver) -> &viewer_ui::ViewState {
    &preview(d).view
}

#[test]
fn the_zoom_box_offers_fit_width_fit_page_and_fixed_zooms() {
    let mut d = driver("viewer-zoom-box", &short_pages());
    assert!(preview(&d).zoom.is_none());
    assert_eq!(view(&d).fit, viewer::Fit::Page);
    let page = preview(&d).page_size();
    d.click("preview_zoom_box");
    d.click("preview_zoom_fit_width");
    d.frames(3);
    assert!(preview(&d).zoom.is_none(), "Fit Width is a fit, not a zoom");
    assert_eq!(view(&d).fit, viewer::Fit::Width);
    // The page is as wide as the view less its margins and the scroll bar.
    let (_, origin) = preview(&d).nav.shown[0];
    let width = d.screen.x - 2. * viewer::MARGIN - viewer::BAR;
    let expected_left =
        viewer::MARGIN + (d.screen.x - viewer::BAR - 2. * viewer::MARGIN - width) / 2.;
    assert!(
        (origin.x - expected_left).abs() < 2.,
        "page starts at {}",
        origin.x
    );
    let z = width / page.x;
    assert!(z > 0.5 && z < 4.);
    d.click("preview_zoom_box");
    d.click("preview_zoom_fit_page");
    d.frames(3);
    assert_eq!((preview(&d).zoom, view(&d).fit), (None, viewer::Fit::Page));
    for (name, zoom) in [
        ("100", 1.),
        ("50", 0.5),
        ("200", 2.),
        ("75", 0.75),
        ("150", 1.5),
    ] {
        d.click("preview_zoom_box");
        d.click(&format!("preview_zoom_{name}"));
        d.frames(2);
        assert_eq!(preview(&d).zoom, Some(zoom), "{name}%");
    }
    // The plus and minus buttons step from there; Fit goes back to a page.
    d.click("preview_zoom_in");
    assert!((preview(&d).zoom.unwrap() - 1.875).abs() < 1e-3);
    d.click("preview_zoom_out");
    d.click("preview_zoom_out");
    assert!((preview(&d).zoom.unwrap() - 1.2).abs() < 1e-3);
    d.click("preview_fit");
    d.frames(2);
    assert!(preview(&d).zoom.is_none());
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn fit_width_follows_the_view_when_the_strip_opens() {
    let mut d = driver("viewer-width-strip", &short_pages());
    d.click("preview_zoom_box");
    d.click("preview_zoom_fit_width");
    d.frames(3);
    let before = preview(&d).nav.shown[0].1.x;
    d.click("preview_thumbs");
    d.frames(3);
    assert!(view(&d).thumbs);
    let after = preview(&d).nav.shown[0].1.x;
    assert!(
        after > before + viewer_ui::STRIP - 10.,
        "{before} to {after}"
    );
    assert_eq!(view(&d).fit, viewer::Fit::Width);
}

#[test]
fn the_page_box_takes_a_page_number() {
    let mut d = driver("viewer-page-box", &short_pages());
    let count = preview(&d).count();
    assert_eq!(view(&d).page_text(), "1");
    type_into(&mut d, "preview_page_box", "3");
    d.key(Key::Enter);
    d.frames(3);
    assert_eq!(preview(&d).page, 2);
    assert!(scroll(&d).y > 0.);
    assert_eq!(view(&d).page_text(), "3", "the box shows the page");
    // Out of range goes to the nearest page; words change nothing.
    type_into(&mut d, "preview_page_box", "99");
    d.key(Key::Enter);
    d.frames(3);
    assert_eq!(preview(&d).page, count - 1);
    type_into(&mut d, "preview_page_box", "x");
    d.key(Key::Enter);
    d.frames(3);
    assert_eq!(preview(&d).page, count - 1);
    // The keys typed in the box do not move the pages behind it.
    d.click("preview_page_box");
    d.key(Key::Home);
    assert_eq!(preview(&d).page, count - 1);
    // Scrolling by hand updates the box.
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.preview.is_none(), "Escape closes");
}

#[test]
fn the_page_strip_shows_thumbnails_follows_the_view_and_jumps() {
    let mut d = driver("viewer-strip", &short_pages());
    d.screen = Vec2::new(1500., 420.);
    d.frames(3);
    assert!(!view(&d).thumbs);
    assert!(!d.has("preview_thumb_0"));
    d.click("preview_thumbs");
    d.frames(3);
    assert!(view(&d).thumbs && d.has("preview_thumb_0"));
    // The thumbnails of the pages built so far fill in while the view rests.
    d.frames(8);
    assert!(preview(&d).has_page(0));
    // A click jumps to the page.
    d.click("preview_thumb_1");
    d.frames(3);
    assert_eq!(preview(&d).page, 1);
    assert!(scroll(&d).y > 0.);
    // Wheel over the strip scrolls the strip, not the pages.
    let page_scroll = scroll(&d).y;
    let strip_at = d.rect("preview_thumb_1").center();
    let _ = d.frame(vec![
        egui::Event::PointerMoved(strip_at),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: Vec2::new(0., -2.),
            modifiers: Default::default(),
        },
    ]);
    d.frames(5);
    assert!(view(&d).thumb_scroll > 0., "the strip scrolled");
    assert_eq!(scroll(&d).y, page_scroll, "and the pages did not");
    // The strip keeps the current page in sight as the pages scroll.
    d.key(Key::End);
    d.frames(4);
    let last = preview(&d).count() - 1;
    let r = d.rect(&format!("preview_thumb_{last}"));
    assert!(
        r.bottom() <= d.screen.y - 20. + 1.,
        "the last thumbnail is in view: {r:?}"
    );
    d.key(Key::Home);
    d.frames(4);
    assert!(view(&d).thumb_scroll < 1.);
    // Turned off again, the strip goes and the pages take its room.
    d.click("preview_thumbs");
    d.frames(3);
    assert!(!d.has("preview_thumb_0"));
}

#[test]
fn find_looks_through_every_page_and_steps_between_matches() {
    let mut d = driver("viewer-find", &short_pages());
    let count = preview(&d).count();
    // Ctrl+F is the viewer's, not the table's Find dialog.
    d.key_with(Key::F, ctrl());
    d.frames(2);
    assert!(view(&d).find.open);
    assert!(
        d.app.grid.dialogs.find.is_none(),
        "the table's Find stayed shut"
    );
    type_into(&mut d, "preview_find_text", "install");
    // One page is searched a frame.
    for _ in 0..count + 8 {
        d.frames(1);
    }
    let f = &view(&d).find;
    assert!(
        !f.searching(count),
        "the search finished: {}",
        f.status(count)
    );
    assert!(f.matches.len() >= 2, "{} matches", f.matches.len());
    let pages: Vec<usize> = f.matches.iter().map(|m| m.page).collect();
    assert!(
        pages.windows(2).all(|w| w[0] <= w[1]),
        "in page order {pages:?}"
    );
    assert_eq!(f.current, Some(0));
    // The first match is scrolled to.
    d.frames(3);
    let first_page = view(&d).find.matches[0].page;
    assert!(preview(&d).nav.shown.iter().any(|(k, _)| *k == first_page));
    // Pages were searched without being kept.
    assert!(preview(&d).built() < count, "{} built", preview(&d).built());
    // Case does not matter; next and previous wrap.
    type_into(&mut d, "preview_find_text", "INSTALL");
    for _ in 0..count + 8 {
        d.frames(1);
    }
    let n = view(&d).find.matches.len();
    assert!(n >= 2);
    d.click("preview_find_next");
    assert_eq!(view(&d).find.current, Some(1));
    d.frames(3);
    let at = view(&d).find.matches[1];
    assert!(preview(&d).nav.shown.iter().any(|(k, _)| *k == at.page));
    d.click("preview_find_prev");
    d.click("preview_find_prev");
    assert_eq!(view(&d).find.current, Some(n - 1), "wraps back");
    d.click("preview_find_next");
    assert_eq!(view(&d).find.current, Some(0));
    // Nothing to find says so.
    type_into(&mut d, "preview_find_text", "zzzzqqq");
    for _ in 0..count + 8 {
        d.frames(1);
    }
    assert_eq!(view(&d).find.status(count), "No matches");
    // Escape closes the Find bar first and the preview second.
    d.key(Key::Escape);
    d.frames(2);
    assert!(!view(&d).find.open && d.app.print.preview.is_some());
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.preview.is_none());
}

#[test]
fn find_in_page_boxes_the_matches_inside_their_text_runs() {
    let pages = report_pages(2);
    let hits = viewer_ui::find_in_page(1, &pages[1], "PAGE 2");
    assert_eq!(hits.len(), 1);
    let m = hits[0];
    assert_eq!(m.page, 1);
    // "Report page 2": "page 2" is the last 6 of 13 characters of a 200 px run.
    assert!(
        (m.rect.left() - (60. + 200. * 7. / 13.)).abs() < 0.01,
        "{:?}",
        m.rect
    );
    assert!((m.rect.right() - 260.).abs() < 0.01);
    assert!(m.rect.top() < 80. && m.rect.bottom() > 80.);
    assert!(viewer_ui::find_in_page(0, &pages[0], "page 2").is_empty());
    assert_eq!(viewer_ui::find_in_page(0, &pages[0], "e").len(), 2);
    assert!(viewer_ui::find_in_page(0, &pages[0], "").is_empty());
}

#[test]
fn the_zoom_mode_and_the_strip_are_remembered() {
    let mut d = driver("viewer-prefs", &short_pages());
    d.click("preview_zoom_box");
    d.click("preview_zoom_fit_width");
    d.click("preview_thumbs");
    d.frames(3);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.print.preview.is_none());
    d.click("tool_print_preview");
    settle(&mut d);
    assert_eq!((preview(&d).zoom, view(&d).fit), (None, viewer::Fit::Width));
    assert!(view(&d).thumbs, "the strip comes back");
    // A fixed zoom is remembered too, and a closed strip stays closed.
    d.click("preview_zoom_box");
    d.click("preview_zoom_150");
    d.click("preview_thumbs");
    d.frames(3);
    d.click("preview_close");
    d.frames(2);
    d.click("tool_print_preview");
    settle(&mut d);
    assert_eq!(preview(&d).zoom, Some(1.5));
    assert!(!view(&d).thumbs);
    // The report preview opens the same way.
    d.key(Key::Escape);
    d.frames(2);
    open_report(&mut d, 3);
    d.frames(3);
    assert_eq!(report_preview(&d).zoom, Some(1.5));
    assert!(!report_preview(&d).view.thumbs);
    d.click("report_preview_thumbs");
    d.frames(2);
    d.click("report_preview_zoom_box");
    d.click("report_preview_zoom_fit_page");
    d.frames(2);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.page_preview.is_none());
    d.click("tool_print_preview");
    settle(&mut d);
    assert_eq!((preview(&d).zoom, view(&d).fit), (None, viewer::Fit::Page));
    assert!(view(&d).thumbs, "the report preview left the strip open");
}

#[test]
fn the_report_preview_has_the_same_viewer() {
    let mut d = Driver::new("viewer-report", openxer_xer::starter());
    d.home();
    open_report(&mut d, 6);
    d.frames(4);
    for name in [
        "report_preview_zoom_box",
        "report_preview_zoom_in",
        "report_preview_zoom_out",
        "report_preview_fit",
        "report_preview_thumbs",
        "report_preview_find",
        "report_preview_page_box",
        "report_preview_first",
        "report_preview_prev",
        "report_preview_next",
        "report_preview_last",
        "report_preview_save",
        "report_preview_html",
        "report_preview_text",
        "report_preview_close",
    ] {
        assert!(d.has(name), "{name} is on the toolbar");
    }
    // Page box, zoom box and strip.
    type_into(&mut d, "report_preview_page_box", "4");
    d.key(Key::Enter);
    d.frames(3);
    assert_eq!(report_preview(&d).page, 3);
    d.click("report_preview_zoom_box");
    d.click("report_preview_zoom_fit_width");
    d.frames(3);
    assert_eq!(report_preview(&d).view.fit, viewer::Fit::Width);
    d.click("report_preview_thumbs");
    d.frames(3);
    d.click("report_preview_thumb_1");
    d.frames(3);
    assert_eq!(report_preview(&d).page, 1);
    // Find, with the text of the pages.
    d.key_with(Key::F, ctrl());
    d.frames(2);
    type_into(&mut d, "report_preview_find_text", "page 5");
    d.frames(3);
    assert_eq!(report_preview(&d).view.find.matches.len(), 1);
    d.frames(3);
    assert!(report_preview(&d).nav.shown.iter().any(|(k, _)| *k == 4));
    d.key(Key::Escape);
    d.frames(2);
    assert!(
        d.app.reports.page_preview.is_some(),
        "Escape closed the Find bar"
    );
    // Export HTML and Text write the report's own files.
    let mut p = d.app.reports.page_preview.take().unwrap();
    let (html, text) = (d.dir.join("out.html"), d.dir.join("out.txt"));
    d.app.export_report_preview(&mut p, true, &html);
    d.app.export_report_preview(&mut p, false, &text);
    assert_eq!(
        std::fs::read_to_string(&html).unwrap(),
        "<html>report</html>"
    );
    // Text is UTF-8 with a byte order mark, so a spreadsheet reads it right.
    assert_eq!(
        std::fs::read_to_string(&text).unwrap(),
        "\u{feff}report text"
    );
    assert!(p.note.as_deref().is_some_and(|n| n.starts_with("Saved")));
    d.app.reports.page_preview = Some(p);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.page_preview.is_none());
}

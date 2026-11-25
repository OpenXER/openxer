//! The movable splitter between the two panels of the Relationships tab
//! (and the Steps, Notebook and Feedback tabs, which share it).
use super::headless_tests::{cleanup, temp_db};
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use std::path::{Path, PathBuf};

struct Session {
    ctx: egui::Context,
    app: App,
    time: f64,
    painter: SoftwarePainter,
}

impl Session {
    fn open(path: &Path) -> Self {
        let store = Store::open(path.to_path_buf()).unwrap();
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let app = App::new(&ctx, store, project);
        let mut s = Session {
            ctx,
            app,
            time: 0.,
            painter: SoftwarePainter::default(),
        };
        // An activity with a predecessor and a successor.
        let model = &s.app.project.model;
        let i = (0..model.activities.len())
            .find(|&i| {
                model.links.iter().any(|l| l.succ == i) && model.links.iter().any(|l| l.pred == i)
            })
            .unwrap();
        s.app.selected = Some(i);
        for _ in 0..3 {
            s.step(vec![]);
        }
        s.click("detail_tab_relationships");
        s
    }

    fn step(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.1;
        let app = &mut self.app;
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
                time: Some(self.time),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
        self.painter.ingest(&output);
        output
    }

    fn rect(&self, action: &str) -> Rect {
        let [l, t, r, b] = self.app.action_rects[action];
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
    }

    fn press(&mut self, pos: Pos2, pressed: bool) {
        self.step(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]);
    }

    fn click(&mut self, action: &str) {
        let pos = self.rect(action).center();
        self.press(pos, true);
        self.press(pos, false);
        self.step(vec![egui::Event::PointerGone]);
        self.step(vec![]);
    }

    /// Drag the Relationships splitter by `dx`.
    fn drag(&mut self, dx: f32) {
        let from = self.rect("detail_split_relationships").center();
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.press(from, true);
        for k in 1..=4 {
            let pos = from + Vec2::new(dx * k as f32 / 4., 0.);
            self.step(vec![egui::Event::PointerMoved(pos)]);
        }
        self.press(from + Vec2::new(dx, 0.), false);
        self.step(vec![egui::Event::PointerGone]);
        self.step(vec![]);
    }

    fn share(&self) -> Option<f32> {
        self.app.detail.splits.get("relationships").copied()
    }

    fn bar_x(&self) -> f32 {
        self.rect("detail_split_relationships").left()
    }

    fn render(&mut self, name: &str) {
        let output = self.step(vec![]);
        let (image, _) = self.painter.rasterize(
            &self.ctx,
            output.shapes,
            output.pixels_per_point,
            Vec2::new(1500., 900.),
        );
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("artifacts")
            .join("status-tab");
        std::fs::create_dir_all(&dir).unwrap();
        image.save(dir.join(name)).unwrap();
    }
}

#[test]
fn split_clamp_keeps_both_minimums() {
    assert_eq!(split_clamp(1000., 100., [300., 300.]), 300.);
    assert_eq!(split_clamp(1000., 900., [300., 300.]), 700.);
    assert_eq!(split_clamp(1000., 450., [300., 300.]), 450.);
    // Too narrow for both: an even split.
    assert_eq!(split_clamp(500., 100., [300., 300.]), 250.);
}

#[test]
fn relationships_splitter_drags_clamps_resets_and_persists() {
    let path = temp_db("split");
    let mut s = Session::open(&path);
    let x0 = s.bar_x();
    assert!(s.share().is_none(), "unmoved until dragged");

    // Dragging moves the splitter: the left panel grows by what the right
    // panel loses.
    s.drag(-120.);
    let x1 = s.bar_x();
    assert!((x1 - (x0 - 120.)).abs() <= 2., "{x0} -> {x1}");
    let share = s.share().expect("drag stores the position");

    // Each side keeps its minimum: dragging far either way stops there.
    s.drag(-3000.);
    let lo = s.share().unwrap();
    s.drag(3000.);
    let hi = s.share().unwrap();
    let w = schedule_tabs::RELATIONSHIP_PANEL_MIN / lo;
    assert!(
        (hi * w - (w - schedule_tabs::RELATIONSHIP_PANEL_MIN)).abs() < 1.5,
        "lo {lo} hi {hi} width {w}"
    );
    assert!(lo < share && share < hi);

    // A double-click splits it 50/50.
    let at = s.rect("detail_split_relationships").center();
    for _ in 0..2 {
        s.press(at, true);
        s.press(at, false);
    }
    s.step(vec![]);
    assert!((s.share().unwrap() - 0.5).abs() < 0.01, "{:?}", s.share());

    // Kept across activities and tab switches.
    s.drag(-80.);
    let kept = s.bar_x();
    let other = s
        .app
        .selected
        .map(|i| (i + 1) % s.app.project.model.activities.len());
    s.app.selected = other;
    s.click("detail_tab_general");
    s.click("detail_tab_relationships");
    assert_eq!(s.bar_x(), kept);

    // And across restarts, as a user preference.
    let stored = s.share();
    drop(s);
    let s = Session::open(&path);
    assert_eq!(s.share(), stored);
    assert_eq!(s.bar_x(), kept);
    drop(s);
    cleanup(path);
}

/// Writes `artifacts/status-tab/relationships-{default,moved}.png`.
/// `cargo.exe test -p openxer-desktop render_relationships_split -- --ignored`.
#[test]
#[ignore = "writes review renders to artifacts/status-tab; run on demand"]
fn render_relationships_split_to_png() {
    let path = temp_db("split-render");
    let mut s = Session::open(&path);
    s.render("relationships-default.png");
    s.drag(-250.);
    s.render("relationships-moved.png");
    drop(s);
    cleanup(path);
}

/// Start and Finish in the relationship grids carry the table's markers:
/// " A" on an actual and the constraint mark on a constrained date.
#[test]
fn relationship_dates_show_the_constraint_and_actual_marks() {
    use crate::grid::marks::{self, End};
    let path = temp_db("split-marks");
    let mut s = Session::open(&path);
    let i = s.app.selected.unwrap();
    let model = &s.app.project.model;
    let succ = model.links.iter().find(|l| l.pred == i).unwrap().succ;
    let pred = model.links.iter().find(|l| l.succ == i).unwrap().pred;
    let at = s.app.project.model.activities[succ].early_start;
    let a = &mut s.app.project.model.activities[succ];
    a.cstr = openxer_engine::model::CstrType::StartOnAfter;
    a.cstr_date = at;
    s.app.project.model.activities[pred].act_start = Some(0);
    let output = s.step(vec![]);
    let shown = super::headless_tests::texts(&output);
    let origin = s.app.project.model.origin_serial;
    let leveled = s.app.project.model.options.do_level;
    let a = &s.app.project.model.activities[succ];
    assert_eq!(marks::mark(a, End::Start), format::DateMark::Constrained);
    let start = format::date_marked(
        origin,
        crate::util::displayed_start(a, leveled),
        format::DateMark::Constrained,
    );
    assert!(start.ends_with(" *"));
    assert!(shown.contains(&start), "{start} not in {shown:?}");
    let p = &s.app.project.model.activities[pred];
    let actual = format::date_marked(
        origin,
        crate::util::displayed_start(p, leveled),
        format::DateMark::Actual,
    );
    assert!(shown.contains(&actual), "{actual} not in {shown:?}");
    drop(s);
    cleanup(path);
}

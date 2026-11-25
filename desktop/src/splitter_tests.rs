//! The table/Gantt splitter and the extent of band and selection fills,
//! checked on headless renders at the reference client size.
use crate::grid::layout::{ColumnSetting, GroupLevel};
use crate::grid::Row;
use crate::headless_render::SoftwarePainter;
use crate::*;
use std::path::PathBuf;

/// Measured splitter columns, left to right (top to bottom when horizontal).
const SPLITTER_PIXELS: [[u8; 3]; 6] = [
    [0xA0; 3], [0xFF; 3], [0xF0; 3], [0xF0; 3], [0xA0; 3], [0xA0; 3],
];

struct Scene {
    path: PathBuf,
    ctx: egui::Context,
    app: App,
    painter: SoftwarePainter,
    time: f64,
}

impl Scene {
    /// The starter project grouped by WBS with two narrow columns, so the
    /// table pane is far wider than its columns.
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "openxer-splitter-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut store = Store::open(path.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        app.grid.layout.columns = [("activity_id", 139.), ("activity_name", 100.)]
            .into_iter()
            .map(|(id, w)| ColumnSetting::new(id, w))
            .collect();
        app.grid.layout.group_sort.levels = vec![GroupLevel {
            column: "wbs".into(),
            to_level: None,
            indent: true,
            interval: String::new(),
        }];
        let mut painter = SoftwarePainter::default();
        painter.screen = Some(Vec2::new(1920., 1009.));
        let mut scene = Self {
            path,
            ctx,
            app,
            painter,
            time: 0.,
        };
        scene.frames(3);
        scene
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.step(vec![]);
        }
    }

    fn step(&mut self, events: Vec<egui::Event>) {
        let _ = self
            .painter
            .frame(&mut self.app, &self.ctx, &mut self.time, events, false);
    }

    fn image(&mut self) -> image::RgbaImage {
        self.painter
            .frame(&mut self.app, &self.ctx, &mut self.time, vec![], true)
            .unwrap()
            .0
    }

    fn rect(&self, action: &str) -> Rect {
        let [l, t, r, b] = self.app.action_rects[action];
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn px(image: &image::RgbaImage, x: f32, y: f32) -> [u8; 3] {
    let p = image.get_pixel(x as u32, y as u32).0;
    [p[0], p[1], p[2]]
}

fn artifacts() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/splitter");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_table_gantt_splitter_has_the_measured_pixels_and_bands_stop_at_the_last_column() {
    let mut s = Scene::new("pixels");
    let second = s
        .app
        .rows()
        .iter()
        .filter_map(|r| match r {
            Row::Activity(i) => Some(*i),
            _ => None,
        })
        .nth(1)
        .unwrap();
    s.app.selected = Some(second);
    s.frames(3);
    let image = s.image();
    image
        .save(artifacts().join("table-gantt-splitter.png"))
        .unwrap();

    let header = s.rect("grid_header_activity_id");
    let left = header.left() - 4.;
    let columns_right = left + 239.;
    let splitter = s.rect("splitter");
    assert_eq!(splitter.width(), 6., "splitter width");
    let x = splitter.left();
    assert!(
        x > columns_right + 100.,
        "the table pane is wider than its columns: pane ends at {x}, columns at {columns_right}"
    );
    // (a) The measured sequence across the splitter, at several heights.
    for y in [header.top() + 10., header.bottom() + 100., 700.] {
        let got: Vec<[u8; 3]> = (0..6).map(|n| px(&image, x + n as f32, y)).collect();
        assert_eq!(got, SPLITTER_PIXELS, "splitter pixels at y {y}");
    }
    // The Gantt starts right after the splitter and the table before it.
    assert_eq!(s.app.gantt_left, x + 6., "Gantt starts after the splitter");
    let chart_right = s.app.gantt_left + s.app.chart_width;
    assert!(
        chart_right > 1800. && chart_right <= 1920.,
        "chart width spans the pane after the splitter: ends at {chart_right}"
    );
    assert_eq!(px(&image, x - 1., 700.), [255; 3], "table side is white");
    // (b) Band and selected rows against the last column.
    let rows = s.app.rows();
    let tops = s.app.grid_row_tops(&rows);
    let bands = s.app.grid.bands.borrow().clone();
    let (band_top, _) = rows
        .iter()
        .zip(&tops)
        .find(|(r, _)| matches!(r, Row::Group(b) if bands[*b].level == 0))
        .map(|(_, t)| *t)
        .unwrap();
    let (sel_top, _) = rows
        .iter()
        .zip(&tops)
        .find(|(r, _)| matches!(r, Row::Activity(i) if Some(*i) == s.app.selected))
        .map(|(_, t)| *t)
        .unwrap();
    let blue = crate::theme::bytes(crate::theme::NAVY);
    let select = [0, 0x78, 0xD7];
    assert_eq!(px(&image, columns_right - 12., band_top + 2.), blue, "band");
    assert_eq!(
        px(&image, columns_right - 12., sel_top + 1.),
        select,
        "selected row"
    );
    for dx in [2., 20., 150.] {
        assert_eq!(
            px(&image, columns_right + dx, band_top + 2.),
            [255; 3],
            "band row after the last column (+{dx})"
        );
        assert_eq!(
            px(&image, columns_right + dx, sel_top + 1.),
            [255; 3],
            "selected row after the last column (+{dx})"
        );
    }
    assert_eq!(
        px(&image, x - 4., band_top + 2.),
        [255; 3],
        "band row just left of the splitter"
    );
    assert_eq!(
        px(&image, columns_right + 20., header.top() + 10.),
        [0xF0; 3],
        "header strip after the last column"
    );
}

#[test]
fn the_bottom_table_and_gantt_use_the_same_splitter_and_fill_extent() {
    let mut s = Scene::new("bottom");
    s.app.show_details = true;
    s.app.set_bottom(Some(panes::BottomView::Gantt));
    s.app.panes.grid.columns = s.app.grid.layout.columns.clone();
    s.frames(4);
    let image = s.image();
    image.save(artifacts().join("bottom-splitter.png")).unwrap();
    // The horizontal splitter above the bottom pane, same sequence top to bottom.
    let rows: Vec<u32> = (300..900u32)
        .filter(|&y| {
            (0..6).all(|n| px(&image, 300., (y + n) as f32) == SPLITTER_PIXELS[n as usize])
        })
        .collect();
    assert_eq!(rows.len(), 1, "one horizontal splitter, found at {rows:?}");
    assert_eq!(
        px(&image, 900., rows[0] as f32 + 2.),
        [0xF0; 3],
        "face of the horizontal splitter over the Gantt"
    );
    // The bottom pane's splitter: the lowest place the sequence appears.
    let y = 960.;
    let found: Vec<u32> = (0..1900u32)
        .filter(|&x| (0..6).all(|n| px(&image, (x + n) as f32, y) == SPLITTER_PIXELS[n as usize]))
        .collect();
    assert!(!found.is_empty(), "no splitter in the bottom pane at y {y}");
    let x = found[0] as f32;
    assert_eq!(
        px(&image, x - 1., y),
        [255; 3],
        "bottom table side is white"
    );
    // Nothing but white between the last column and the splitter.
    let columns_right = s.rect("grid_header_activity_id").left() - 4. + 239.;
    assert_eq!(px(&image, (x - 3.).max(columns_right + 3.), y), [255; 3]);
}

#[test]
fn dragging_the_splitter_moves_the_table_gantt_boundary() {
    let mut s = Scene::new("drag");
    let before = s.rect("splitter");
    let start = before.center();
    let target = Pos2::new(start.x - 200., start.y);
    s.step(vec![egui::Event::PointerMoved(start)]);
    s.step(vec![egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }]);
    for n in 1..=4 {
        let pos = Pos2::new(start.x - 50. * n as f32, start.y);
        s.step(vec![egui::Event::PointerMoved(pos)]);
    }
    s.step(vec![egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    s.frames(2);
    let after = s.rect("splitter");
    assert!(
        (after.center().x - target.x).abs() <= 6.,
        "splitter follows the pointer: {} -> {} (wanted {})",
        before.center().x,
        after.center().x,
        target.x
    );
    assert_eq!(s.app.gantt_left, after.left() + 6.);
}

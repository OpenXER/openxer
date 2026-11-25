//! Review render of the activity table: the starter project in a 1080p
//! window with fixed column widths and six nested band levels, written to
//! the ignored `artifacts/review/grid/` folder.
use super::layout::{ColumnSetting, GroupLevel};
use super::Row;
use crate::headless_render::SoftwarePainter;
use crate::*;
use std::path::PathBuf;

/// Client x where the Gantt starts in the render: the table pane (with its scroll
/// bar) ends 6 px earlier, where the splitter begins.
const CHART_LEFT: f32 = 658.;

#[test]
fn render_activity_grid_for_review() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/grid");
    std::fs::create_dir_all(&output).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-grid-review-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        painter.screen = Some(Vec2::new(1920., 1009.));
        let layout = &mut app.grid.layout;
        layout.columns = [
            ("activity_id", 139.),
            ("activity_name", 409.),
            ("original_duration", 51.),
            ("budgeted_labor_units", 65.),
            ("start", 99.),
            ("finish", 103.),
        ]
        .into_iter()
        .map(|(id, w)| ColumnSetting::new(id, w))
        .collect();
        // Six nested band levels, a deep WBS.
        layout.group_sort.levels = [
            "wbs",
            "activity_status",
            "activity_type",
            "calendar",
            "duration_type",
            "percent_complete_type",
        ]
        .into_iter()
        .map(|c| GroupLevel {
            column: c.into(),
            to_level: None,
            indent: true,
            interval: String::new(),
        })
        .collect();
        // The reference's first two band fonts (12 and 11 pt) fix the band row
        // heights the geometry is compared at; the colors are OpenXER's own.
        for (level, size) in [(0, 12.), (1, 11.)] {
            let band = &mut layout.group_sort.bands[level];
            band.font_size = size;
            band.row_height = super::layout::band_row_height(size);
        }
        // Sorted by a column outside the pinned area.
        layout.group_sort.sort = vec![super::layout::SortKey {
            column: "start".into(),
            descending: false,
        }];
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        // Put the table/Gantt splitter at CHART_LEFT.
        let left = app.action_rects["grid_header_activity_id"][0] - 4.;
        let width = 1920. - left;
        app.grid_fraction = (CHART_LEFT - crate::activities::SPLITTER - left) / width;
        let second = app
            .rows()
            .iter()
            .filter_map(|r| match r {
                Row::Activity(i) => Some(*i),
                _ => None,
            })
            .nth(1)
            .unwrap();
        app.selected = Some(second);
        app.grid.focus_column = Some("activity_id".into());
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(output.join("openxer-grid.png")).unwrap();
        // Geometry for the comparison script: header, row kinds and tops.
        let header = app.action_rects["grid_header_activity_id"];
        let rows = app.rows();
        let bands = app.grid.bands.borrow().clone();
        let tops = app.grid_row_tops(&rows);
        let list: Vec<_> = rows
            .iter()
            .zip(&tops)
            .map(|(row, (top, height))| match *row {
                Row::Group(b) => serde_json::json!({
                    "kind": "band", "level": bands[b].level, "top": top, "height": height,
                }),
                Row::Activity(i) => serde_json::json!({
                    "kind": "activity", "selected": app.selected == Some(i),
                    "top": top, "height": height,
                }),
            })
            .collect();
        let geometry = serde_json::json!({
            "table_left": header[0] - 4.,
            "header_top": header[1],
            "header_bottom": header[3],
            "rows": list,
        });
        std::fs::write(
            output.join("openxer-grid.json"),
            serde_json::to_string_pretty(&geometry).unwrap(),
        )
        .unwrap();
        // The drawing, pixel by pixel.
        let px = |x: f32, y: f32| {
            let p = image.get_pixel(x as u32, y as u32).0;
            [p[0], p[1], p[2]]
        };
        let (left, head_top, head_bottom) = (header[0] - 4., header[1], header[3]);
        const LINE: [u8; 3] = [0xC0; 3];
        assert_eq!(px(left + 1., head_top), [255; 3], "header top edge");
        assert_eq!(px(left, head_top + 10.), [255; 3], "header left edge");
        assert_eq!(px(left + 138., head_top + 10.), [0; 3], "divider");
        assert_eq!(px(left + 139., head_top + 10.), [255; 3], "divider edge");
        assert_eq!(
            px(left + 300., head_bottom - 1.),
            [0; 3],
            "header bottom line"
        );
        assert_eq!(px(left + 300., head_top + 30.), [0xF0; 3], "header face");
        let bands_style = super::layout::default_bands();
        let row = |want: &dyn Fn(&Row) -> bool| {
            rows.iter()
                .zip(&tops)
                .find(|(r, _)| want(r))
                .map(|(_, t)| *t)
                .unwrap()
        };
        let (top, height) = row(&|r| matches!(r, Row::Group(b) if bands[*b].level == 0));
        assert_eq!((top, height), (head_bottom, 24.), "level 1 band");
        assert_eq!(px(left + 1., top + 1.), bands_style[0].background);
        assert_eq!(px(left + 2., top + 8.), [0xA0; 3], "expand box border");
        assert_eq!(px(left + 6., top + 12.), [0; 3], "minus sign");
        assert_eq!(px(left + 300., top + 23.), LINE, "band bottom line");
        assert_eq!(
            px(left + 547., top + 5.),
            LINE,
            "line after the caption span"
        );
        let (_, height) = row(&|r| matches!(r, Row::Group(b) if bands[*b].level == 1));
        assert_eq!(height, 22., "level 2 band");
        let (top, height) = row(&|r| matches!(r, Row::Activity(i) if Some(*i) != app.selected));
        assert_eq!(height, 18.);
        for (k, style) in bands_style.iter().take(6).enumerate() {
            let x = left + 5. * k as f32;
            assert_eq!(px(x + 1., top + 17.), style.background, "strip {k}");
            assert_eq!(px(x + 4., top + 5.), LINE, "strip line {k}");
        }
        assert_eq!(px(left + 100., top + 17.), LINE, "row line");
        assert_eq!(px(left + 138., top + 5.), LINE, "column line");
        assert_eq!(px(left + 100., top + 1.), [255; 3], "cell");
        let (top, _) = row(&|r| matches!(r, Row::Activity(i) if Some(*i) == app.selected));
        assert_eq!(px(left + 1., top + 17.), [0, 0x78, 0xD7], "selected strip");
        assert_eq!(
            px(left + 48., top + 5.),
            crate::theme::bytes(crate::theme::AMBER),
            "focus outline"
        );
        assert_eq!(px(left + 125., top + 5.), [0, 0x78, 0xD7], "selected cell");
        assert_eq!(px(left + 100., top + 17.), LINE, "selected row line");
        // The in-place editors: a text cell and a choice cell.
        let catalog = super::columns::catalog(&app.project);
        for (name, column) in [("text", "activity_name"), ("choice", "activity_type")] {
            let col = super::columns::find(&catalog, column).unwrap().clone();
            if !app.grid.layout.columns.iter().any(|c| c.id == column) {
                app.grid
                    .layout
                    .columns
                    .insert(2, ColumnSetting::new(column, 110.));
            }
            app.begin_edit(second, &col);
            for _ in 0..2 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            let (image, _) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            image
                .save(output.join(format!("openxer-grid-edit-{name}.png")))
                .unwrap();
            app.edit_cell = None;
            app.grid
                .layout
                .columns
                .retain(|c| c.id != "activity_type" || column != "activity_type");
        }
        println!("Grid review render: {}", output.display());
    }
    let _ = std::fs::remove_file(database);
}

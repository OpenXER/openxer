//! Headless tests of the Gantt options: real frames, real clicks on the
//! rendered controls, no native window and no system pointer.
use super::settings::*;
use super::*;
use crate::headless_render::SoftwarePainter;

fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
) -> egui::FullOutput {
    *time += 0.1;
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    )
}

fn press(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    button: egui::PointerButton,
    time: &mut f64,
) {
    for pressed in [true, false] {
        let _ = frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            time,
        );
    }
    let _ = frame(app, ctx, vec![egui::Event::PointerGone], time);
    let _ = frame(app, ctx, vec![], time);
}

fn rect_of(app: &App, action: &str) -> Rect {
    let [l, t, r, b] = app.action_rects[action];
    Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
}

fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    let pos = rect_of(app, action).center();
    press(app, ctx, pos, egui::PointerButton::Primary, time);
}

fn click_text(app: &mut App, ctx: &egui::Context, text: &str, time: &mut f64) {
    let output = frame(app, ctx, vec![], time);
    let pos = output
        .shapes
        .iter()
        .rev()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) if shape.galley.text() == text => {
                let rect = shape.visual_bounding_rect().intersect(clipped.clip_rect);
                rect.is_positive().then(|| rect.center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("No visible text {text:?}"));
    press(app, ctx, pos, egui::PointerButton::Primary, time);
}

/// Wheel-scrolls the Bars list until the control is fully visible.
fn reveal(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    for _ in 0..40 {
        let list = rect_of(app, "bars_list");
        // The grid draws only the rows in view: scroll down until it shows.
        let delta = match app.action_rects.get(action).copied() {
            Some([l, t, r, b]) => {
                let target = Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b));
                if list.contains_rect(target) {
                    return;
                }
                if target.top() < list.top() {
                    30.
                } else {
                    -30.
                }
            }
            None => -30.,
        };
        let _ = frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(list.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0., delta),
                    modifiers: Default::default(),
                },
            ],
            time,
        );
        for _ in 0..3 {
            let _ = frame(app, ctx, vec![], time);
        }
    }
    panic!("{action} never scrolled into view");
}

fn settle(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    for _ in 0..3 {
        let _ = frame(app, ctx, vec![], time);
    }
}

fn test_app(name: &str) -> (PathBuf, egui::Context, App, f64) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-gantt-{name}-{}-{unique}.db",
        std::process::id()
    ));
    let store = Store::open(path.clone()).unwrap();
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    settle(&mut app, &ctx, &mut time);
    (path, ctx, app, time)
}

/// Filled rectangles of one color inside the chart body.
fn chart_rects(app: &App, output: &egui::FullOutput, color: Color32) -> Vec<Rect> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(shape)
                if shape.fill == color && shape.rect.left() >= app.gantt_left - 1. =>
            {
                let visible = shape.rect.intersect(clipped.clip_rect);
                visible.is_positive().then_some(shape.rect)
            }
            _ => None,
        })
        .collect()
}

fn chart_texts(app: &App, output: &egui::FullOutput) -> Vec<(String, Rect)> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => {
                let rect = shape.visual_bounding_rect().intersect(clipped.clip_rect);
                (rect.is_positive() && rect.left() >= app.gantt_left)
                    .then(|| (shape.galley.text().to_owned(), rect))
            }
            _ => None,
        })
        .collect()
}

fn body_point(app: &App) -> Pos2 {
    Pos2::new(app.gantt_left + 200., 600.)
}

#[test]
fn bars_list_columns_fit_their_text_and_the_date_columns_shrink() {
    use super::dialogs::bar_list_widths;
    // Room for all: Name, Timescale and Filter get what they need, Name the
    // rest; the empty date columns are at most 72 px.
    let w = bar_list_widths([100., 110., 95., 100., 120.]);
    assert_eq!((w[2], w[5]), (110., 120.));
    assert_eq!((w[3], w[4]), (72., 72.));
    assert!(w[1] >= 100.);
    assert_eq!(w.iter().sum::<f32>(), 609.);
    // Too much text: shared in proportion, the list keeps its width.
    let w = bar_list_widths([200., 200., 95., 100., 200.]);
    assert!((w.iter().sum::<f32>() - 609.).abs() < 0.5);
    assert!(w[1] < 200. && w[2] < 200. && w[5] < 200.);
}

#[test]
fn default_bars_paint_measured_shapes_and_toggling_a_bar_removes_it() {
    let (path, ctx, mut app, mut time) = test_app("toggle");
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let green = crate::theme::color(crate::theme::BLUE);
    let red = crate::theme::color(crate::theme::CRITICAL);
    let remaining = chart_rects(&app, &output, green);
    assert!(
        !remaining.is_empty(),
        "Non-critical remaining work is drawn in the theme blue"
    );
    let rows: Vec<Rect> = app.bar_geometry.iter().map(|b| b.row).collect();
    for bar in &remaining {
        assert_eq!(bar.height(), 9., "Activity bars are 9 px tall");
        assert!(
            rows.iter().any(|row| bar.top() - row.top() == 3.),
            "Bars sit 3 px below their row top"
        );
    }
    assert!(!chart_rects(&app, &output, red).is_empty());

    // Right-click the chart body, then Bars... from the Gantt menu.
    let at = body_point(&app);
    press(
        &mut app,
        &ctx,
        at,
        egui::PointerButton::Secondary,
        &mut time,
    );
    assert!(
        app.gantt_ui.menu.is_some(),
        "Right-click opens the Gantt menu"
    );
    click(&mut app, &ctx, "gantt_menu_bars", &mut time);
    assert!(app.gantt_ui.menu.is_none());
    assert!(app.gantt_ui.bars.is_some(), "Bars... opens the Bars dialog");
    settle(&mut app, &ctx, &mut time);
    let index = app
        .gantt
        .bars
        .iter()
        .position(|b| b.name == "Remaining Work")
        .unwrap();
    reveal(&mut app, &ctx, &format!("bars_display_{index}"), &mut time);
    click(&mut app, &ctx, &format!("bars_display_{index}"), &mut time);
    assert!(app.gantt.bars[index].display, "A draft until OK");
    click(&mut app, &ctx, "bars_ok", &mut time);
    assert!(app.gantt_ui.bars.is_none());
    assert!(!app.gantt.bars[index].display);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    assert!(
        chart_rects(&app, &output, green).is_empty(),
        "A bar switched off is no longer painted"
    );
    assert!(!chart_rects(&app, &output, red).is_empty());
    assert_eq!(app.bar_geometry.len(), 14, "Relationship handles stay");
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_label_added_in_the_bars_dialog_shows_on_the_chart() {
    let (path, ctx, mut app, mut time) = test_app("labels");
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let texts = chart_texts(&app, &output);
    assert!(
        texts
            .iter()
            .any(|(t, _)| t == "Site investigation & survey"),
        "Default labels show the activity name right of the bar"
    );
    app.open_bars_dialog();
    settle(&mut app, &ctx, &mut time);
    let index = app
        .gantt
        .bars
        .iter()
        .position(|b| b.name == "Current Bar Labels")
        .unwrap();
    reveal(&mut app, &ctx, &format!("bars_row_{index}"), &mut time);
    click(&mut app, &ctx, &format!("bars_row_{index}"), &mut time);
    assert_eq!(app.gantt_ui.bars.as_ref().unwrap().selected, index);
    click(&mut app, &ctx, "bars_tab_labels", &mut time);
    click(&mut app, &ctx, "bars_label_add", &mut time);
    click(&mut app, &ctx, "bars_ok", &mut time);
    assert_eq!(app.gantt.bars[index].labels.len(), 2);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let texts = chart_texts(&app, &output);
    let (_, label) = texts
        .iter()
        .find(|(t, _)| t == "Site investigation & survey, RB-1010")
        .expect("Labels at one position are joined");
    let bar = app.bar_geometry.iter().find(|b| b.task_id == 1010).unwrap();
    assert!(label.left() > bar.finish.x + 5. && label.left() < bar.finish.x + 14.);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn timescale_dialog_changes_band_labels_and_three_lines_adds_a_band() {
    let (path, ctx, mut app, mut time) = test_app("timescale");
    click(&mut app, &ctx, "timescale", &mut time);
    click_text(&mut app, &ctx, "Automatic (follows zoom)", &mut time);
    click_text(&mut app, &ctx, "Week/Day 2", &mut time);
    click(&mut app, &ctx, "ts_ok", &mut time);
    assert_eq!(
        (app.view.upper, app.view.lower),
        (timescale::Unit::Week, timescale::Unit::Day)
    );
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let lower = rect_of(&app, "timescale_zoom");
    let labels: Vec<String> = chart_texts(&app, &output)
        .into_iter()
        .filter(|(_, r)| lower.contains_rect(*r))
        .map(|(t, _)| t)
        .collect();
    assert!(labels.len() > 5);
    assert!(
        labels
            .iter()
            .all(|t| ["S", "M", "T", "W", "F"].contains(&t.as_str())),
        "Week/Day 2 labels days with one letter: {labels:?}"
    );

    click(&mut app, &ctx, "timescale", &mut time);
    click(&mut app, &ctx, "ts_three_lines", &mut time);
    click(&mut app, &ctx, "ts_ok", &mut time);
    assert!(app.gantt.timescale.three_lines);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let upper = rect_of(&app, "timescale_pan");
    let lower = rect_of(&app, "timescale_zoom");
    assert_eq!(upper.height() + lower.height(), 52.);
    assert_eq!(lower.height(), 18., "Bands of 17, 17 and 18 px");
    let months: Vec<String> = chart_texts(&app, &output)
        .into_iter()
        .filter(|(_, r)| r.bottom() <= upper.top() + 17.)
        .map(|(t, _)| t)
        .collect();
    assert!(
        months
            .iter()
            .any(|t| t.starts_with("November") || t.starts_with("December")),
        "The added top band shows months: {months:?}"
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn curtain_draws_only_inside_its_dates_and_hides() {
    let (path, ctx, mut app, mut time) = test_app("curtain");
    let origin = app.project.model.origin_serial;
    let dd = app.project.model.project.data_date.unwrap();
    app.open_curtain(None, dd + 3 * DAY);
    {
        let draft = app.gantt_ui.curtain.as_mut().unwrap();
        draft.curtain.pattern = Pattern::Solid;
        draft.curtain.color = Rgb(0xff, 0x80, 0xc0);
        draft.start_text = date_string(origin, Some(dd + 3 * DAY));
        draft.finish_text = date_string(origin, Some(dd + 6 * DAY));
    }
    settle(&mut app, &ctx, &mut time);
    click(&mut app, &ctx, "curtain_ok", &mut time);
    assert!(app.gantt_ui.curtain.is_none());
    assert_eq!(app.gantt.curtains.len(), 1);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let axis = timescale::TimeScale::new(
        app.timeline_origin as f64,
        app.pixels_per_day as f64 * 7.,
        app.gantt_left,
    );
    let (x0, x1) = (axis.x((dd + 3 * DAY) as f64), axis.x((dd + 6 * DAY) as f64));
    let pink = chart_rects(&app, &output, Color32::from_rgb(0xff, 0x80, 0xc0));
    assert!(!pink.is_empty(), "The curtain is painted");
    for rect in &pink {
        assert!(rect.left() >= x0 - 0.5 && rect.right() <= x1 + 0.5);
    }
    // A finish before the start is refused and the dialog stays open.
    app.open_curtain(Some(0), dd);
    app.gantt_ui.curtain.as_mut().unwrap().finish_text = date_string(origin, Some(dd));
    settle(&mut app, &ctx, &mut time);
    click(&mut app, &ctx, "curtain_ok", &mut time);
    assert!(app.gantt_ui.curtain.as_ref().unwrap().error.is_some());
    click(&mut app, &ctx, "curtain_cancel", &mut time);
    app.gantt.curtains[0].display = false;
    let output = frame(&mut app, &ctx, vec![], &mut time);
    assert!(chart_rects(&app, &output, Color32::from_rgb(0xff, 0x80, 0xc0)).is_empty());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn saved_settings_persist_and_invalid_saved_settings_are_rejected() {
    let (path, ctx, mut app, mut time) = test_app("persist");
    let mut settings = app.gantt.clone();
    settings.options.progress_line.show = true;
    settings.bars[2].display = true;
    settings.bars[2].row = 2;
    app.apply_gantt(settings.clone()).unwrap();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.gantt_row_height(), app.view.row_height + 12.);
    let mut invalid = settings.clone();
    invalid.bars[0].row = 9;
    assert!(app.apply_gantt(invalid).is_err());
    assert_eq!(app.gantt, settings, "A rejected apply changes nothing");
    drop(app);

    let reopened = App::new(
        &ctx,
        Store::open(path.clone()).unwrap(),
        openxer_xer::starter(),
    );
    assert_eq!(reopened.gantt, settings);
    drop(reopened);

    let store = Store::open(path.clone()).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&settings.to_json()).unwrap();
    json["bars"][0]["row"] = 9.into();
    store
        .set_preference("gantt_settings_v1", &json.to_string())
        .unwrap();
    let reopened = App::new(&ctx, store, openxer_xer::starter());
    assert_eq!(reopened.gantt, GanttSettings::default());
    assert!(reopened.status.contains("rejected"));
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

/// Software renders of the Gantt and every Gantt dialog, saved to the
/// ignored `artifacts/` folder for review.
#[test]
fn render_gantt_and_dialogs_to_png() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (path, _, app, _) = test_app("render");
    drop(app);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let mut app = App::new(&ctx, Store::open(path.clone()).unwrap(), project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    let mut shot = |app: &mut App, name: &str, time: &mut f64| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, time, vec![], true).unwrap();
        image.save(artifacts.join(name)).unwrap();
        println!(
            "Software-rendered Gantt: {}",
            artifacts.join(name).display()
        );
    };
    shot(&mut app, "gantt-default.png", &mut time);
    app.open_bars_dialog();
    for (tab, name) in [
        (dialogs::BarsTab::Style, "gantt-bars-style.png"),
        (dialogs::BarsTab::Settings, "gantt-bars-settings.png"),
        (dialogs::BarsTab::Labels, "gantt-bars-labels.png"),
    ] {
        let draft = app.gantt_ui.bars.as_mut().unwrap();
        draft.tab = tab;
        draft.selected = 7;
        shot(&mut app, name, &mut time);
    }
    app.gantt_ui.bars = None;
    app.open_chart_options();
    for (tab, name) in [
        (dialogs::OptionsTab::General, "gantt-options-general.png"),
        (
            dialogs::OptionsTab::Collapsed,
            "gantt-options-collapsed.png",
        ),
        (dialogs::OptionsTab::DataDate, "gantt-options-datadate.png"),
        (
            dialogs::OptionsTab::SightLines,
            "gantt-options-sightlines.png",
        ),
        (
            dialogs::OptionsTab::ProgressLine,
            "gantt-options-progress.png",
        ),
    ] {
        app.gantt_ui.options.as_mut().unwrap().tab = tab;
        shot(&mut app, name, &mut time);
    }
    app.gantt_ui.options = None;
    app.open_timescale_dialog();
    shot(&mut app, "gantt-timescale.png", &mut time);
    app.gantt_ui.timescale = None;
    let dd = app.project.model.project.data_date.unwrap();
    app.open_curtain(None, dd + 7 * DAY);
    shot(&mut app, "gantt-curtain.png", &mut time);
    app.gantt_ui.curtain = None;
    let at = body_point(&app);
    app.open_gantt_menu(at);
    shot(&mut app, "gantt-context-menu.png", &mut time);
    app.gantt_ui.menu = None;
    let mut settings = app.gantt.clone();
    settings.timescale.three_lines = true;
    settings.options.progress_line.show = true;
    settings.options.show_legend = true;
    settings.curtains.push(Curtain {
        display: true,
        start: dd + 14 * DAY,
        finish: dd + 21 * DAY,
        color: Rgb(0xc0, 0xc0, 0xc0),
        pattern: Pattern::ForwardDiagonal,
    });
    for bar in &mut settings.bars {
        if bar.name == "Float Bar" {
            bar.display = true;
        }
    }
    app.apply_gantt(settings).unwrap();
    app.collapsed.insert(format!("w{}|", app.project.wbs[1].id));
    shot(&mut app, "gantt-options-applied.png", &mut time);
    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Minutes from the model origin for a calendar date and time.
fn minute_at(app: &App, (y, m, d): (i32, u32, u32), (h, mi): (i64, i64)) -> i64 {
    let days = chrono::NaiveDate::from_ymd_opt(y, m, d)
        .unwrap()
        .signed_duration_since(chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap())
        .num_days();
    (days - app.project.model.origin_serial) * DAY + h * 60 + mi
}

/// A synthetic scene at a fixed zoom, timescale phase and set of bar dates
/// (activity names are the starter project's own), whose pixel positions
/// are pinned. Writes the render and its anchors to the ignored
/// `artifacts/review/gantt/` folder.
#[test]
fn review_scene_renders_the_pinned_gantt_layout() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/gantt");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (path, _, app, _) = test_app("review-scene");
    drop(app);
    // A fresh context, so the software painter sees every font texture.
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let mut app = App::new(&ctx, Store::open(path.clone()).unwrap(), project);
    let mut time = 0.;
    let code = |app: &App, code: &str| {
        app.project
            .model
            .activities
            .iter()
            .position(|a| a.task_code == code)
            .unwrap()
    };
    let done = |app: &mut App, code_name: &str, start: i64, finish: i64, kind: Option<TaskType>| {
        let i = code(app, code_name);
        let a = &mut app.project.model.activities[i];
        if let Some(kind) = kind {
            a.task_type = kind;
        }
        a.status = openxer_engine::model::StatusType::Completed;
        a.act_start = Some(start);
        a.act_finish = Some(finish);
        a.early_start = Some(start);
        a.early_finish = Some(finish);
        a.rem = 0;
    };
    let start = minute_at(&app, (2026, 9, 21), (19, 10));
    let finish_24 = minute_at(&app, (2026, 9, 24), (5, 59));
    let finish_25 = minute_at(&app, (2026, 9, 25), (5, 59));
    let milestone = minute_at(&app, (2026, 9, 23), (8, 0));
    done(&mut app, "RB-1000", milestone, milestone, None);
    done(&mut app, "RB-1010", start, finish_24, None);
    done(&mut app, "RB-1020", start, finish_25, None);
    done(&mut app, "RB-1030", finish_25, finish_25, None);
    let loe_finish = minute_at(&app, (2026, 10, 14), (12, 0));
    done(&mut app, "RB-1040", start, loe_finish, Some(TaskType::Loe));
    app.project.model.project.data_date = Some(minute_at(&app, (2026, 10, 30), (6, 0)));
    app.gantt.options.show_relationships = false;
    // The splitter takes 6 px before the chart, so the table pane is 6 px
    // narrower to keep the chart at its pinned left edge.
    app.grid_fraction = 0.296;
    app.view.automatic_units = true;
    // Reference zoom, fitted to its ten week ticks: 101.55 px per week, with
    // Sunday 20-Sep-26 21.27 px right of the chart edge.
    app.pixels_per_day = 101.55 / 7.;
    let sep20 = minute_at(&app, (2026, 9, 20), (0, 0));
    app.timeline_origin = sep20 - 2111;
    app.update_timescale_units();
    app.selected = Some(code(&app, "RB-1010"));
    let mut painter = SoftwarePainter::default();
    let mut shoot = |app: &mut App, name: &str, time: &mut f64| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, time, vec![], true).unwrap();
        image.save(artifacts.join(name)).unwrap();
    };
    shoot(&mut app, "openxer-scene.png", &mut time);
    let header = rect_of(&app, "timescale_pan");
    let rows: Vec<_> = app
        .bar_geometry
        .iter()
        .map(|b| {
            serde_json::json!({
                "code": app.project.model.activities[b.activity].task_code,
                "top": b.row.top(),
                "height": b.row.height(),
            })
        })
        .collect();
    let axis = TimeScale::new(
        app.timeline_origin as f64,
        f64::from(app.pixels_per_day) * 7.,
        app.gantt_left,
    );
    let x = |minute: i64| axis.x(minute as f64);
    let anchors = serde_json::json!({
        "chart_left": app.gantt_left,
        "chart_right": app.gantt_left + app.chart_width,
        "header_top": header.top(),
        "header_height": app.gantt_header_height(),
        "rows": rows,
        "x": {
            "sep20": x(sep20),
            "start": x(start),
            "finish_24": x(finish_24),
            "finish_25": x(finish_25),
            "milestone": x(milestone),
            "loe_finish": x(loe_finish),
            "data_date": x(app.project.model.project.data_date.unwrap()),
        },
    });
    std::fs::write(
        artifacts.join("openxer-scene.json"),
        serde_json::to_string_pretty(&anchors).unwrap(),
    )
    .unwrap();
    assert_eq!(x(sep20), app.gantt_left + 21.);
    // The other zoom rungs, each pinned: a date whose boundary is on show,
    // its column and the sub-pixel phase.
    for (ppw, (y, m, d), column, phase, bands) in [
        // (pixels per week, date, column, phase, bands)
        (
            28.,
            (2026, 6, 1),
            6.,
            0.5,
            (timescale::Unit::Month, timescale::Unit::Week),
        ),
        (
            13.,
            (2026, 1, 1),
            51.,
            0.93,
            (timescale::Unit::Quarter, timescale::Unit::Month),
        ),
        (
            200.,
            (2026, 10, 11),
            46.,
            0.2,
            (timescale::Unit::Week, timescale::Unit::Day),
        ),
    ] {
        app.pixels_per_day = (ppw / 7.) as f32;
        let anchor = minute_at(&app, (y, m, d), (0, 0));
        let (ppw, column, phase): (f64, f64, f64) = (ppw, column, phase);
        app.timeline_origin = anchor - ((column + phase) * 10080. / ppw).round() as i64;
        app.update_timescale_units();
        assert_eq!((app.view.upper, app.view.lower), bands);
        let name = format!("openxer-zoom-{ppw}.png");
        shoot(&mut app, &name, &mut time);
        let axis = TimeScale::new(app.timeline_origin as f64, ppw, app.gantt_left);
        assert_eq!(axis.x(anchor as f64), app.gantt_left + column as f32);
    }
    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

// --------------------------------------------------- timescale label fitting

/// Header labels of one frame, from the upper band(s) down: text and bounds.
fn header_labels(app: &App, output: &egui::FullOutput) -> Vec<Vec<(String, Rect)>> {
    let pan = rect_of(app, "timescale_pan");
    let zoom = rect_of(app, "timescale_zoom");
    let header = pan.union(zoom);
    let mut bands: Vec<Vec<(String, Rect)>> = vec![vec![]; app.timescale_bands().len()];
    let height = header.height() / bands.len() as f32;
    for clipped in &output.shapes {
        let egui::Shape::Text(shape) = &clipped.shape else {
            continue;
        };
        let rect = shape.galley.rect.translate(shape.pos.to_vec2());
        if rect.center().x < header.left() || !header.contains(rect.center()) {
            continue;
        }
        let band = (((rect.center().y - header.top()) / height) as usize).min(bands.len() - 1);
        bands[band].push((shape.galley.text().to_string(), rect));
    }
    bands
}

/// The Gantt at a zoom with its bands set, settled, and the last frame's header.
fn header_at(
    app: &mut App,
    ctx: &egui::Context,
    time: &mut f64,
    pixels_per_week: f64,
    units: (timescale::Unit, timescale::Unit),
    first: (i32, u32, u32),
) -> Vec<Vec<(String, Rect)>> {
    app.pixels_per_day = (pixels_per_week / 7.) as f32;
    app.timeline_origin = minute_at(app, first, (0, 0));
    (app.view.upper, app.view.lower) = units;
    app.view.automatic_units = false;
    let mut output = frame(app, ctx, vec![], time);
    for _ in 0..2 {
        output = frame(app, ctx, vec![], time);
    }
    header_labels(app, &output)
}

#[test]
fn header_labels_fit_whole_and_never_overlap_at_every_zoom() {
    use timescale::Unit;
    let (path, ctx, mut app, mut time) = test_app("label-fit");
    let right = app.gantt_left + app.chart_width;
    let mut labels = 0;
    for hour_24 in [true, false] {
        app.formats.dates.time = if hour_24 {
            crate::format::TimeDisplay::Hour24
        } else {
            crate::format::TimeDisplay::Hour12
        };
        for (scale, units) in [
            (2016. * 2., (Unit::Day, Unit::Hour)),
            (4032., (Unit::Day, Unit::Hour)),
            (1800., (Unit::Day, Unit::Hour)),
            (1260., (Unit::Day, Unit::Hour)),
            (900., (Unit::Day, Unit::Hour)),
            (450., (Unit::Day, Unit::Hour)),
            (1000., (Unit::Week, Unit::Day)),
            (400., (Unit::Week, Unit::Day)),
            (200., (Unit::Week, Unit::Day)),
            (100., (Unit::Week, Unit::Day)),
            (60., (Unit::Week, Unit::Day)),
            (28., (Unit::Month, Unit::Week)),
            (20., (Unit::Month, Unit::Week)),
            (13., (Unit::Quarter, Unit::Month)),
            (8., (Unit::Quarter, Unit::Month)),
            (5., (Unit::Year, Unit::Quarter)),
            (1., (Unit::Year, Unit::Quarter)),
        ] {
            let bands = header_at(&mut app, &ctx, &mut time, scale, units, (2026, 9, 17));
            for (n, band) in bands.iter().enumerate() {
                let mut sorted = band.clone();
                sorted.sort_by(|a, b| a.1.left().total_cmp(&b.1.left()));
                for pair in sorted.windows(2) {
                    assert!(
                        pair[0].1.right() <= pair[1].1.left(),
                        "{scale} band {n}: {:?} overlaps {:?}",
                        pair[0].0,
                        pair[1].0
                    );
                }
                for (text, rect) in &sorted {
                    labels += 1;
                    if units.1 == Unit::Hour && n == 1 {
                        // Hour labels are never cut by the chart's edges.
                        assert!(
                            rect.left() >= app.gantt_left && rect.right() <= right,
                            "{scale}: {text:?} {rect:?} cut at the chart edge"
                        );
                    }
                }
            }
            if units.1 == Unit::Hour {
                assert!(!bands[1].is_empty(), "{scale} px per week: hour labels");
            }
        }
    }
    assert!(labels > 300, "{labels}");
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn hour_labels_follow_the_time_format_and_days_carry_weekday_names() {
    use timescale::Unit;
    let (path, ctx, mut app, mut time) = test_app("label-names");
    let hour_band = |app: &mut App, ctx: &egui::Context, time: &mut f64, scale: f64| {
        header_at(
            app,
            ctx,
            time,
            scale,
            (Unit::Day, Unit::Hour),
            (2026, 9, 17),
        )
    };
    let texts = |band: &Vec<(String, Rect)>| -> Vec<String> {
        band.iter().map(|(t, _)| t.clone()).collect()
    };
    // 24-hour: roomy cells read 06:00, tight ones 06, tighter ones skip hours.
    app.formats.dates.time = crate::format::TimeDisplay::Hour24;
    let roomy = hour_band(&mut app, &ctx, &mut time, 24. * 7. * 40.);
    assert!(
        texts(&roomy[1]).contains(&"06:00".to_string()),
        "{:?}",
        texts(&roomy[1])
    );
    let tight = hour_band(&mut app, &ctx, &mut time, 24. * 7. * 19.);
    assert!(
        texts(&tight[1]).contains(&"06".to_string()),
        "{:?}",
        texts(&tight[1])
    );
    let reported = hour_band(&mut app, &ctx, &mut time, 24. * 7. * 7.5);
    let shown = texts(&reported[1]);
    assert!(
        shown.iter().all(|t| t.len() == 2 || t.len() == 5),
        "{shown:?}"
    );
    assert!(
        shown.len() < 24 * 3 && shown.contains(&"12".to_string())
            || shown.contains(&"12:00".to_string()),
        "{shown:?}"
    );
    // The day band above names the weekday with the date.
    let upper = texts(&reported[0]);
    assert!(
        upper.iter().any(|t| t.starts_with("Thursday Sep 17")
            || t.starts_with("Thu")
            || t.starts_with("Thr")),
        "{upper:?}"
    );
    // 12-hour: 6 AM when roomy, 6a when tight.
    app.formats.dates.time = crate::format::TimeDisplay::Hour12;
    let roomy = hour_band(&mut app, &ctx, &mut time, 24. * 7. * 40.);
    assert!(
        texts(&roomy[1]).contains(&"6 AM".to_string()),
        "{:?}",
        texts(&roomy[1])
    );
    let tight = hour_band(&mut app, &ctx, &mut time, 24. * 7. * 19.);
    assert!(
        texts(&tight[1]).contains(&"6a".to_string()),
        "{:?}",
        texts(&tight[1])
    );
    // Day cells at week zoom: weekday names shorten with the width.
    let day = |app: &mut App, ctx: &egui::Context, time: &mut f64, scale: f64| {
        let bands = header_at(
            app,
            ctx,
            time,
            scale,
            (Unit::Week, Unit::Day),
            (2026, 9, 17),
        );
        texts(&bands[1])
    };
    let wide = day(&mut app, &ctx, &mut time, 7. * 130.);
    assert!(wide.iter().any(|t| t == "Thursday 17"), "{wide:?}");
    let medium = day(&mut app, &ctx, &mut time, 7. * 42.);
    assert!(medium.iter().any(|t| t == "Thr 17"), "{medium:?}");
    let names = day(&mut app, &ctx, &mut time, 200.);
    assert!(names.iter().any(|t| t == "Thr"), "{names:?}");
    let letters = day(&mut app, &ctx, &mut time, 100.);
    assert!(
        letters.iter().all(|t| t.chars().count() == 1),
        "{letters:?}"
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Software renders of the timescale at hour, day, week and month zoom (and a
/// 12-hour hour band) in the ignored `artifacts/review/gantt/` folder,
/// cropped to the header, for looking at the label fitting.
#[test]
fn render_timescale_labels_at_every_zoom_for_review() {
    use timescale::Unit;
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/gantt");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (path, _, app, _) = test_app("label-renders");
    drop(app);
    // A fresh context, so the software painter sees every font texture.
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let mut app = App::new(&ctx, Store::open(path.clone()).unwrap(), project);
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    for (name, scale, units, hour_24) in [
        ("labels-hour-roomy", 4032., (Unit::Day, Unit::Hour), true),
        ("labels-hour-tight", 1260., (Unit::Day, Unit::Hour), true),
        ("labels-hour-12", 1700., (Unit::Day, Unit::Hour), false),
        ("labels-day-wide", 7. * 130., (Unit::Week, Unit::Day), true),
        ("labels-day-names", 200., (Unit::Week, Unit::Day), true),
        ("labels-week", 100., (Unit::Week, Unit::Day), true),
        ("labels-month", 28., (Unit::Month, Unit::Week), true),
        ("labels-quarter", 13., (Unit::Quarter, Unit::Month), true),
    ] {
        app.formats.dates.time = if hour_24 {
            crate::format::TimeDisplay::Hour24
        } else {
            crate::format::TimeDisplay::Hour12
        };
        app.pixels_per_day = (scale / 7.) as f32;
        app.timeline_origin = minute_at(&app, (2026, 9, 17), (0, 0)) + 5 * 60;
        (app.view.upper, app.view.lower) = units;
        app.view.automatic_units = false;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        let header = rect_of(&app, "timescale_pan").union(rect_of(&app, "timescale_zoom"));
        let crop = image::imageops::crop_imm(
            &image,
            header.left() as u32,
            (header.top() - 2.) as u32,
            (header.width() as u32).min(image.width() - header.left() as u32),
            header.height() as u32 + 4,
        )
        .to_image();
        // Enlarged so the label pixels are easy to read.
        let big = image::imageops::resize(
            &crop,
            crop.width() * 2,
            crop.height() * 2,
            image::imageops::FilterType::Nearest,
        );
        big.save(artifacts.join(format!("{name}.png"))).unwrap();
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Bars, bar labels, relationship lines and the progress line read percent
/// complete, durations and critical exactly as the table columns do.
#[test]
fn bar_facts_and_labels_match_the_table_columns() {
    use crate::details::setters::Edit;
    use crate::grid::columns::{self, Field};
    let (path, _ctx, mut app, _time) = test_app("columns");
    let i = app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_type == TaskType::Task && a.rem > 0)
        .unwrap();
    let start = app.project.model.activities[i].early_start.unwrap();
    app.edit_activity(i, Edit::PercentType("CP_Drtn")).unwrap();
    app.edit_activity(i, Edit::ActualStart(Some(start)))
        .unwrap();
    app.edit_activity(i, Edit::DurationPercent(50.)).unwrap();
    assert_eq!(
        app.project.details[&app.project.model.activities[i].task_id].percent,
        0.
    );
    let ix = app.shared_index();
    let a = app.project.model.activities[i].clone();
    // Duration % type: 50% done, though the physical percent is 0.
    assert_eq!(app.subject(&ix, i, &a).percent, 50.);
    let column = |f: Field| {
        columns::display(
            &app.project,
            Some(i),
            &columns::field(&app.project, &ix, i, f),
        )
    };
    assert_eq!(
        app.label_text(&ix, LabelField::PercentComplete, &a, i),
        "50%"
    );
    for (label, field) in [
        (LabelField::OriginalDuration, Field::OriginalDuration),
        (LabelField::RemainingDuration, Field::RemainingDuration),
        (LabelField::TotalFloat, Field::TotalFloat),
        (LabelField::FreeFloat, Field::FreeFloat),
    ] {
        let text = app.label_text(&ix, label, &a, i);
        assert_eq!(text, column(field), "{label:?}");
        assert!(!text.contains(".0"), "{label:?} is not raw hours: {text}");
    }
    // Critical follows the project's definition: a float threshold that
    // takes in every open activity makes every open bar critical.
    let pid = app.project.model.project.proj_id.to_string();
    let project = app.project.document.table_mut("PROJECT");
    let row = project
        .rows
        .iter()
        .position(|r| project.value(r, "proj_id") == pid)
        .unwrap();
    project.set(row, "critical_drtn_hr_cnt", "100000".into());
    let ix = app.shared_index();
    assert_eq!(ix.critical, (false, 100000.));
    for (k, a) in app.project.model.activities.iter().enumerate() {
        assert_eq!(
            app.subject(&ix, k, a).critical,
            columns::is_critical(a, ix.critical)
        );
        assert_eq!(
            app.subject(&ix, k, a).critical,
            !a.is_completed() && a.total_float.is_some()
        );
    }
    let _ = std::fs::remove_file(path);
}

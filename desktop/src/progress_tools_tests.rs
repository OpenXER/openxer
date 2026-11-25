use super::*;
use crate::headless_render::SoftwarePainter;
use openxer_engine::calendar::{CalendarAdapter, WorkCalendar};

// ------------------------------------------------------------------ helpers

fn temp_db(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openxer-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn frames(app: &mut App, ctx: &egui::Context, painter: &mut SoftwarePainter, time: &mut f64) {
    for _ in 0..3 {
        let _ = painter.frame(app, ctx, time, vec![], false);
    }
}

fn open_app(
    project: ProjectData,
    name: &str,
) -> (egui::Context, App, SoftwarePainter, f64, PathBuf) {
    let path = temp_db(name);
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    frames(&mut app, &ctx, &mut painter, &mut time);
    (ctx, app, painter, time, path)
}

type Table<'a> = (&'a str, &'a [&'a str], &'a [&'a [&'a str]]);

fn xer(tables: &[Table]) -> String {
    let mut s = "ERMHDR\t25.12\t2026-11-04\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD\n".to_string();
    for (name, fields, rows) in tables {
        s += &format!("%T\t{name}\n%F\t{}\n", fields.join("\t"));
        for r in *rows {
            s += &format!("%R\t{}\n", r.join("\t"));
        }
    }
    s + "%E\n"
}

/// Three parallel 2-day activities that each need the one available unit of
/// the same resource, so leveling must run them one after another.
fn leveling_project() -> ProjectData {
    let week: String = (1..=7)
        .map(|d| {
            if (2..=6).contains(&d) {
                format!("(0||{d}()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))")
            } else {
                format!("(0||{d}()())")
            }
        })
        .collect();
    let calendar = format!("(0||CalendarData()((0||DaysOfWeek()({week}))(0||Exceptions()())))");
    let task_fields: &[&str] = &[
        "task_id",
        "proj_id",
        "wbs_id",
        "clndr_id",
        "task_code",
        "task_name",
        "task_type",
        "status_code",
        "duration_type",
        "complete_pct_type",
        "priority_type",
        "remain_drtn_hr_cnt",
        "target_drtn_hr_cnt",
        "target_start_date",
        "target_end_date",
    ];
    let assignment_fields: &[&str] = &[
        "taskrsrc_id",
        "task_id",
        "proj_id",
        "rsrc_id",
        "remain_qty",
        "target_qty",
        "remain_qty_per_hr",
        "target_qty_per_hr",
        "cost_per_qty",
        "target_cost",
        "remain_cost",
        "rate_type",
        "rsrc_type",
        "cost_per_qty_source_type",
    ];
    let text = xer(&[
        (
            "PROJECT",
            &[
                "proj_id",
                "proj_short_name",
                "last_recalc_date",
                "plan_start_date",
                "clndr_id",
            ],
            &[&["1", "LEVEL", "2026-11-02 08:00", "2026-11-02 08:00", "1"]],
        ),
        (
            "CALENDAR",
            &["clndr_id", "clndr_name", "clndr_data"],
            &[&["1", "Standard", &calendar]],
        ),
        (
            "SCHEDOPTIONS",
            &[
                "schedoptions_id",
                "proj_id",
                "sched_retained_logic",
                "level_float_thrs_cnt",
                "level_outer_assign_flag",
                "level_outer_assign_priority",
                "level_over_alloc_pct",
                "level_within_float_flag",
                "level_keep_sched_date_flag",
                "level_all_rsrc_flag",
                "LevelPriorityList",
            ],
            &[&[
                "1",
                "1",
                "Y",
                "0",
                "N",
                "5",
                "0",
                "N",
                "Y",
                "Y",
                "priority_type,ASC_BY_FIELD/ASC\u{7f}\u{7f}",
            ]],
        ),
        (
            "PROJWBS",
            &[
                "wbs_id",
                "proj_id",
                "wbs_short_name",
                "wbs_name",
                "seq_num",
                "proj_node_flag",
            ],
            &[&["10", "1", "LEVEL", "Level", "1", "Y"]],
        ),
        (
            "RSRC",
            &[
                "rsrc_id",
                "rsrc_short_name",
                "rsrc_name",
                "rsrc_type",
                "active_flag",
                "ot_factor",
            ],
            &[&["7", "CREW", "Crew", "RT_Labor", "Y", "1.5"]],
        ),
        (
            "RSRCRATE",
            &[
                "rsrc_rate_id",
                "rsrc_id",
                "max_qty_per_hr",
                "cost_per_qty",
                "start_date",
            ],
            &[&["1", "7", "1", "50", "2000-01-01 00:00"]],
        ),
        (
            "TASK",
            task_fields,
            &[
                &[
                    "100",
                    "1",
                    "10",
                    "1",
                    "A100",
                    "First",
                    "TT_Task",
                    "TK_NotStart",
                    "DT_FixedDUR2",
                    "CP_Drtn",
                    "PT_High",
                    "16",
                    "16",
                    "2026-11-02 08:00",
                    "2026-11-03 17:00",
                ],
                &[
                    "110",
                    "1",
                    "10",
                    "1",
                    "A110",
                    "Second",
                    "TT_Task",
                    "TK_NotStart",
                    "DT_FixedDUR2",
                    "CP_Phys",
                    "PT_Normal",
                    "16",
                    "16",
                    "2026-11-02 08:00",
                    "2026-11-03 17:00",
                ],
                &[
                    "120",
                    "1",
                    "10",
                    "1",
                    "A120",
                    "Third",
                    "TT_Task",
                    "TK_NotStart",
                    "DT_FixedQty",
                    "CP_Drtn",
                    "PT_Low",
                    "16",
                    "16",
                    "2026-11-02 08:00",
                    "2026-11-03 17:00",
                ],
            ],
        ),
        (
            "TASKRSRC",
            assignment_fields,
            &[
                &[
                    "1",
                    "100",
                    "1",
                    "7",
                    "16",
                    "16",
                    "1",
                    "1",
                    "40",
                    "640",
                    "640",
                    "COST_PER_QTY",
                    "RT_Labor",
                    "ST_Rsrc",
                ],
                &[
                    "2",
                    "110",
                    "1",
                    "7",
                    "16",
                    "16",
                    "1",
                    "1",
                    "40",
                    "640",
                    "640",
                    "COST_PER_QTY",
                    "RT_Labor",
                    "ST_Rsrc",
                ],
                &[
                    "3",
                    "120",
                    "1",
                    "7",
                    "16",
                    "16",
                    "1",
                    "1",
                    "40",
                    "640",
                    "640",
                    "COST_PER_QTY",
                    "RT_Labor",
                    "ST_Rsrc",
                ],
            ],
        ),
    ]);
    let doc = Document::parse(&text).unwrap();
    let mut p = ProjectData::from_document(doc, 1, "level-test".into()).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// The engine's leveled result for a project, computed directly.
fn engine_leveled(p: &ProjectData) -> Model {
    let mut next = p.clone();
    next.model.options.do_level = true;
    next.refresh_derived_durations();
    schedule_model(&mut next.model, &next.resources).unwrap();
    next.model
}

fn dates(m: &Model) -> Vec<[Option<i64>; 8]> {
    m.activities
        .iter()
        .map(|a| {
            [
                a.restart,
                a.reend,
                a.early_start,
                a.early_finish,
                a.late_start,
                a.late_finish,
                a.total_float,
                a.free_float,
            ]
        })
        .collect()
}

// ------------------------------------------------------- leveling settings

#[test]
fn priority_list_round_trips_and_rewrites_only_changed_items() {
    let stored = "ACTVCODE_PRIORITY_369|seq_num*,ASC_BY_FIELD/ASC\u{7f}\u{7f}total_float_hr_cnt,/ASC\u{7f}\u{7f}";
    let mut rows = parse_priority_list(stored);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].key, "ACTVCODE_PRIORITY_369");
    assert!(rows[1].ascending);
    assert_eq!(priority_list_text(&rows), stored);
    rows[1].ascending = false;
    rows.push(PriorityRow::new("priority_type", true));
    rows.push(PriorityRow::new("ACTVCODE_AREA", false));
    rows.push(PriorityRow::new("late_start_date", true));
    let text = priority_list_text(&rows);
    assert_eq!(
        text,
        "ACTVCODE_PRIORITY_369|seq_num*,ASC_BY_FIELD/ASC\u{7f}\u{7f}total_float_hr_cnt,/DESC\u{7f}\u{7f}\
         priority_type,ASC_BY_FIELD/ASC\u{7f}\u{7f}ACTVCODE_AREA|seq_num*,ASC_BY_FIELD/DESC\u{7f}\u{7f}\
         late_start_date,/ASC\u{7f}\u{7f}"
    );
    let again = parse_priority_list(&text);
    assert!(same_priorities(&again, &rows));
    // The short stored form without a trailing mark parses too.
    assert_eq!(
        parse_priority_list("priority_type,ASC_BY_FIELD/ASC").len(),
        1
    );
}

#[test]
fn settings_write_only_changed_cells_and_create_a_row_when_missing() {
    let mut p = leveling_project();
    let before = p.document.clone();
    let s = LevelSettings::read(&p, false);
    assert!(s.keep_dates && s.all_resources && !s.within_float && !s.outer_assign);
    assert_eq!(s.over_alloc, "0");
    assert_eq!(s.priorities.len(), 1);
    assert!(!s.write(&mut p).unwrap(), "nothing changed");
    assert_eq!(p.document.text(), before.text());
    let mut s2 = s.clone();
    s2.over_alloc = "25".into();
    s2.priorities
        .push(PriorityRow::new("total_float_hr_cnt", false));
    assert!(s2.write(&mut p).unwrap());
    let read = LevelSettings::read(&p, false);
    assert_eq!(read.over_alloc, "25");
    assert!(same_priorities(&read.priorities, &s2.priorities));
    assert_eq!(
        sched_value(&p.document, 1, "LevelPriorityList").unwrap(),
        "priority_type,ASC_BY_FIELD/ASC\u{7f}\u{7f}total_float_hr_cnt,/DESC\u{7f}\u{7f}"
    );
    // The engine inputs follow the stored settings.
    p.refresh_derived_durations();
    assert_eq!(p.resources.priorities.len(), 2);
    // The over-allocation percentage applies only to leveling within total
    // float; leveling outside float gives the same dates at 25% and 0%.
    assert_eq!(p.resources.limits[&7][0].1, 1.0);
    // Invalid text is rejected before anything is written.
    let mut bad = read.clone();
    bad.over_alloc = "lots".into();
    assert!(bad.write(&mut p).is_err());
    bad.over_alloc = "0".into();
    bad.outer_priority = "9".into();
    assert!(bad.write(&mut p).is_err());
    // A project without a SCHEDOPTIONS row gets one with the new-row values.
    let mut starter = openxer_xer::starter();
    assert!(schedoptions_row(&starter.document, 1).is_none());
    let mut s3 = LevelSettings::read(&starter, false);
    assert!(s3.priorities.is_empty());
    s3.priorities.push(PriorityRow::new("priority_type", true));
    assert!(s3.write(&mut starter).unwrap());
    assert_eq!(
        sched_value(&starter.document, 1, "level_keep_sched_date_flag").as_deref(),
        Some("Y")
    );
    let reread = ProjectData::from_document(
        Document::parse(&starter.updated_document().text()).unwrap(),
        1,
        "x".into(),
    )
    .unwrap();
    assert_eq!(reread.resources.priorities.len(), 1);
    assert!(reread.resources.unsupported.is_empty());
}

#[test]
fn engine_gaps_name_the_options_the_leveler_lacks() {
    let p = leveling_project();
    let choices = priority_choices(&p.document, 1);
    let mut s = LevelSettings::read(&p, false);
    assert!(s.engine_gaps(&choices).is_empty());
    // Leveling within float is built; other projects' assignments and
    // replacing the scheduled dates are not.
    s.within_float = true;
    assert!(s.engine_gaps(&choices).is_empty());
    s.outer_assign = true;
    s.keep_dates = false;
    s.priorities.push(PriorityRow::new("proj_priority", true));
    let gaps = s.engine_gaps(&choices);
    assert_eq!(gaps.len(), 3, "{gaps:?}");
}

// ------------------------------------------------------- Level dialog

#[test]
fn level_dialog_equals_engine_leveling_and_f9_with_leveling_and_undoes() {
    let project = leveling_project();
    let expected = engine_leveled(&project);
    let delayed = delayed_by_leveling(&expected);
    assert_eq!(
        delayed.len(),
        2,
        "two of three activities wait for the crew"
    );

    // Through the dialog.
    let (ctx, mut app, mut painter, mut time, path) = open_app(project.clone(), "level-dialog");
    let before = app.project.model.clone();
    app.open_level_resources();
    frames(&mut app, &ctx, &mut painter, &mut time);
    for key in [
        "level_auto",
        "level_outer",
        "level_keep_dates",
        "level_recalc_costs",
        "level_all_resources",
        "level_select_resources",
        "level_within_float",
        "level_over_alloc",
        "level_add",
        "level_remove",
        "level_run",
        "level_close",
        "level_view_log",
        "level_default",
        "level_help",
    ] {
        assert!(app.action_rects.contains_key(key), "{key}");
    }
    // Shift+F9 opens Level Resources rather than Schedule.
    app.sched.progress.level = None;
    let _ = painter.frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Key {
            key: Key::F9,
            physical_key: Some(Key::F9),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }],
        false,
    );
    frames(&mut app, &ctx, &mut painter, &mut time);
    assert!(app.sched.progress.level.is_some() && !app.sched.schedule_open);
    painter.click_action(&mut app, &ctx, &mut time, "level_run");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(
        app.sched.progress.level.is_none(),
        "Level closes the dialog"
    );
    assert_eq!(dates(&app.project.model), dates(&expected));
    assert!(
        app.project.model.options.do_level,
        "leveled dates are shown"
    );
    assert!(
        app.sched.progress.manual_level,
        "automatic leveling stays off"
    );
    assert!(app.log.iter().any(|l| l.starts_with("\tLeveling")));
    assert!(app
        .log
        .iter()
        .any(|l| l.contains("Activities delayed due to resource leveling") && l.ends_with('2')));
    assert!(app.log.iter().any(|l| l == "\t\tCREW\tCrew"));
    // One undo step restores the unleveled schedule; redo levels again.
    let (undo, _) = app.history.counts();
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.history.counts().0, undo - 1);
    assert_eq!(dates(&app.project.model), dates(&before));
    assert!(app.history.redo(&mut app.project));
    assert_eq!(dates(&app.project.model), dates(&expected));
    // The next F9 drops a manual Level, as automatic leveling is off.
    app.open_schedule_dialog();
    app.run_schedule_dialog();
    assert!(!app.project.model.options.do_level);
    assert_eq!(dates(&app.project.model), dates(&before));
    drop(app);
    let _ = std::fs::remove_file(path);

    // The same answer from F9 with automatic leveling on.
    let (ctx, mut app, mut painter, mut time, path) = open_app(project, "level-f9");
    app.open_level_resources();
    frames(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "level_auto");
    painter.click_action(&mut app, &ctx, &mut time, "level_close");
    assert!(app.project.model.options.do_level);
    assert!(!app.sched.progress.manual_level);
    app.open_options();
    assert!(app.draft_options.as_ref().unwrap().do_level);
    app.show_schedule = false;
    app.draft_options = None;
    app.open_schedule_dialog();
    app.run_schedule_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(dates(&app.project.model), dates(&expected));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn level_failure_rolls_back_and_keeps_the_dialog_open() {
    let mut project = leveling_project();
    set_sched_value(
        &mut project.document,
        1,
        "level_outer_assign_flag",
        "Y".into(),
    );
    let (ctx, mut app, mut painter, mut time, path) = open_app(project, "level-fail");
    let before = app.project.clone();
    app.open_level_resources();
    frames(&mut app, &ctx, &mut painter, &mut time);
    assert!(app.sched.progress.level.as_ref().unwrap().outer_assign);
    painter.click_action(&mut app, &ctx, &mut time, "level_run");
    let error = app.error.clone().unwrap();
    assert!(error.contains("other projects"), "{error}");
    assert!(app.sched.progress.level.is_some());
    assert_eq!(app.project.model, before.model);
    assert_eq!(app.project.document.text(), before.document.text());
    // The unsupported option may be turned off, and then leveling runs.
    painter.click_action(&mut app, &ctx, &mut time, "level_outer");
    painter.click_action(&mut app, &ctx, &mut time, "level_run");
    assert!(app.sched.progress.level.is_none(), "{:?}", app.error);
    assert_eq!(
        sched_value(&app.project.document, 1, "level_outer_assign_flag").as_deref(),
        Some("N")
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn priorities_grid_adds_reorders_and_removes() {
    let (ctx, mut app, mut painter, mut time, path) = open_app(leveling_project(), "level-grid");
    app.open_level_resources();
    frames(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "level_add");
    let rows = |app: &App| {
        app.sched
            .progress
            .level
            .as_ref()
            .unwrap()
            .priorities
            .iter()
            .map(|r| r.key.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&app), ["priority_type", "total_float_hr_cnt"]);
    painter.click_action(&mut app, &ctx, &mut time, "level_up");
    assert_eq!(rows(&app), ["total_float_hr_cnt", "priority_type"]);
    painter.click_action(&mut app, &ctx, &mut time, "level_order");
    painter.click_action(&mut app, &ctx, &mut time, "level_order_item_1");
    assert!(!app.sched.progress.level.as_ref().unwrap().priorities[0].ascending);
    painter.click_action(&mut app, &ctx, &mut time, "level_field");
    painter.click_action(&mut app, &ctx, &mut time, "level_field_item_3");
    assert_eq!(rows(&app), ["late_start_date", "priority_type"]);
    painter.click_action(&mut app, &ctx, &mut time, "level_remove");
    assert_eq!(rows(&app), ["priority_type"]);
    painter.click_action(&mut app, &ctx, &mut time, "level_default");
    assert_eq!(app.sched.progress.level.as_ref().unwrap().over_alloc, "0");
    // Esc discards the draft.
    painter.click_action(&mut app, &ctx, &mut time, "level_add");
    let _ = painter.frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        false,
    );
    assert!(app.sched.progress.level.is_none());
    assert_eq!(LevelSettings::read(&app.project, false).priorities.len(), 1);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// The corpus projects (private, skipped when absent): leveling through the
/// dialog gives exactly the engine's leveled dates. Projects whose stored
/// leveling settings the engine rejects must fail the same way both ways.
#[test]
fn level_dialog_matches_engine_on_corpus_projects() {
    let dir = crate::test_corpus::dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("SKIPPED: corpus not found in {}", dir.display());
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "xer"))
        .collect();
    files.sort();
    let mut leveled = 0;
    let mut refused = 0;
    for file in files {
        let Ok(doc) = Document::read(&file) else {
            continue;
        };
        if doc.table("TASKRSRC").is_none() {
            continue;
        }
        for (id, _) in doc.projects() {
            let Ok(mut p) = ProjectData::from_document(doc.clone(), id, file.display().to_string())
            else {
                continue;
            };
            if schedule_model(&mut p.model, &p.resources).is_err() {
                continue;
            }
            let mut direct = p.clone();
            direct.model.options.do_level = true;
            direct.refresh_derived_durations();
            let engine = schedule_model(&mut direct.model, &direct.resources);
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (ctx, mut app, mut painter, mut time, path) = open_app(p, "level-corpus");
            app.open_level_resources();
            frames(&mut app, &ctx, &mut painter, &mut time);
            painter.click_action(&mut app, &ctx, &mut time, "level_run");
            match engine {
                Ok(()) => {
                    assert!(app.error.is_none(), "{name}: {:?}", app.error);
                    assert_eq!(dates(&app.project.model), dates(&direct.model), "{name}");
                    leveled += 1;
                }
                Err(e) => {
                    // The dialog names the unsupported settings before running.
                    let shown = app.error.clone().unwrap_or_default();
                    assert!(shown.contains("does not support"), "{name}: {shown} / {e}");
                    assert!(app.sched.progress.level.is_some(), "{name}");
                    refused += 1;
                }
            }
            drop(app);
            let _ = std::fs::remove_file(path);
        }
    }
    println!(
        "corpus leveling through the dialog: {leveled} leveled, {refused} refused by the engine"
    );
}

// ------------------------------------------------------- progress rules

const ORIGIN: i64 = 46_328; // 2026-11-02, a Monday: day 0 of the test calendar.

fn five_by_eight() -> CalendarAdapter {
    let day = vec![(8 * 60, 12 * 60), (13 * 60, 17 * 60)];
    let template = std::array::from_fn(|d| if d < 5 { day.clone() } else { vec![] });
    CalendarAdapter::new(WorkCalendar::new(
        "5x8".into(),
        false,
        ORIGIN,
        template,
        BTreeMap::new(),
    ))
}

fn at(day: i64, hour: i64) -> i64 {
    day * DAY + hour * 60
}

fn input(status: StatusType, start: i64, finish: i64, rem: i64) -> ProgressInput {
    ProgressInput {
        task_type: TaskType::Task,
        status,
        act_start: None,
        act_finish: None,
        rem,
        original: rem,
        start: Some(start),
        finish: Some(finish),
        duration: DurationKind::DurationAndUnits,
        percent: PercentKind::Duration,
        physical: 0.,
        physical_from_steps: false,
    }
}

#[test]
fn not_started_activities_start_finish_or_wait_across_the_new_data_date() {
    let cal = five_by_eight();
    let old = at(0, 8);
    let new = at(2, 8); // Wednesday 08:00: two working days later.
    let always = RemainingRule::Always;
    // Monday to Friday, 40 h: starts Monday, 24 h left.
    let i = input(StatusType::NotStarted, at(0, 8), at(4, 17), 40 * 60);
    let o = auto_progress(&i, &cal, old, new, always).unwrap();
    assert_eq!(o.status, StatusType::InProgress);
    assert_eq!(o.act_start, Some(at(0, 8)));
    assert_eq!(o.act_finish, None);
    assert_eq!(o.rem, 24 * 60);
    // Finishes Tuesday 17:00, before the new data date: complete.
    let i = input(StatusType::NotStarted, at(0, 8), at(1, 17), 16 * 60);
    let o = auto_progress(&i, &cal, old, new, always).unwrap();
    assert_eq!(o.status, StatusType::Completed);
    assert_eq!(
        (o.act_start, o.act_finish, o.rem),
        (Some(at(0, 8)), Some(at(1, 17)), 0)
    );
    // Starts at the new data date: untouched.
    let i = input(StatusType::NotStarted, new, at(3, 17), 16 * 60);
    assert!(auto_progress(&i, &cal, old, new, always).is_none());
    // A stale start before the current data date starts at the data date.
    let i = input(StatusType::NotStarted, at(-3, 8), at(4, 17), 40 * 60);
    let o = auto_progress(&i, &cal, old, new, always).unwrap();
    assert_eq!(o.act_start, Some(old));
    // A new data date that is not later records no progress.
    let i = input(StatusType::NotStarted, at(0, 8), at(4, 17), 40 * 60);
    assert!(auto_progress(&i, &cal, new, old, always).is_none());
}

#[test]
fn in_progress_and_completed_activities() {
    let cal = five_by_eight();
    let (old, new) = (at(0, 8), at(2, 8));
    let mut i = input(StatusType::InProgress, at(0, 8), at(4, 17), 40 * 60);
    i.act_start = Some(at(-2, 8));
    let o = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(
        (o.status, o.act_start, o.rem),
        (StatusType::InProgress, Some(at(-2, 8)), 24 * 60)
    );
    // Scheduled to finish Tuesday noon: completes on its scheduled finish.
    i.finish = Some(at(1, 12));
    let o = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(
        (o.status, o.act_finish, o.rem),
        (StatusType::Completed, Some(at(1, 12)), 0)
    );
    // Completed activities, LOE and WBS summaries are left alone.
    let mut done = input(StatusType::Completed, at(0, 8), at(1, 17), 0);
    done.act_start = Some(at(-5, 8));
    done.act_finish = Some(at(-4, 17));
    assert!(auto_progress(&done, &cal, old, new, RemainingRule::Always).is_none());
    for t in [TaskType::Loe, TaskType::Wbs] {
        let mut loe = input(StatusType::NotStarted, at(0, 8), at(4, 17), 40 * 60);
        loe.task_type = t;
        assert!(auto_progress(&loe, &cal, old, new, RemainingRule::Always).is_none());
    }
}

#[test]
fn milestones_complete_on_their_date() {
    let cal = five_by_eight();
    let (old, new) = (at(0, 8), at(2, 8));
    let mut m = input(StatusType::NotStarted, at(1, 8), at(1, 8), 0);
    m.task_type = TaskType::Mile;
    let o = auto_progress(&m, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(
        (o.status, o.act_start, o.act_finish),
        (StatusType::Completed, Some(at(1, 8)), Some(at(1, 8)))
    );
    // A start milestone at the new data date has not happened yet.
    m.start = Some(new);
    m.finish = Some(new);
    assert!(auto_progress(&m, &cal, old, new, RemainingRule::Always).is_none());
    // A finish milestone at the new data date has.
    m.task_type = TaskType::FinMile;
    let o = auto_progress(&m, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(o.act_finish, Some(new));
    // A finish milestone later than the new data date waits.
    m.start = Some(at(3, 17));
    m.finish = Some(at(3, 17));
    assert!(auto_progress(&m, &cal, old, new, RemainingRule::Always).is_none());
}

#[test]
fn remaining_duration_follows_each_duration_type() {
    let cal = five_by_eight();
    let (old, new) = (at(0, 8), at(2, 8));
    // The stored remaining (32 h) is shorter than the scheduled bar (40 h),
    // so the two rules give different answers.
    let base = input(StatusType::NotStarted, at(0, 8), at(4, 17), 32 * 60);
    for (code, kind, by_type) in [
        ("DT_FixedQty", DurationKind::Units, 16 * 60),
        ("DT_FixedRate", DurationKind::UnitsTime, 16 * 60),
        ("DT_FixedDrtn", DurationKind::DurationAndUnitsTime, 24 * 60),
        ("DT_FixedDUR2", DurationKind::DurationAndUnits, 24 * 60),
    ] {
        assert_eq!(DurationKind::from_code(code), kind);
        let mut i = base.clone();
        i.duration = kind;
        let always = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
        assert_eq!(always.rem, 24 * 60, "{code} always");
        let typed = auto_progress(&i, &cal, old, new, RemainingRule::DurationType).unwrap();
        assert_eq!(typed.rem, by_type, "{code} by type");
    }
    // In progress: Fixed Units consume the period's work from the remaining.
    let mut i = base.clone();
    i.status = StatusType::InProgress;
    i.act_start = Some(at(-1, 8));
    i.duration = DurationKind::Units;
    let o = auto_progress(&i, &cal, old, new, RemainingRule::DurationType).unwrap();
    assert_eq!(o.rem, 16 * 60);
}

#[test]
fn percent_complete_types() {
    let cal = five_by_eight();
    let (old, new) = (at(0, 8), at(2, 8));
    let mut i = input(StatusType::NotStarted, at(0, 8), at(4, 17), 40 * 60);
    assert_eq!(PercentKind::from_code("CP_Phys"), PercentKind::Physical);
    assert_eq!(PercentKind::from_code("CP_Units"), PercentKind::Units);
    assert_eq!(PercentKind::from_code("CP_Drtn"), PercentKind::Duration);
    // Duration percent is derived from the durations; nothing stored changes.
    let o = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(o.physical, 0.);
    // Physical percent follows the duration progress.
    i.percent = PercentKind::Physical;
    let o = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(o.physical, 40.);
    // ...unless steps drive it.
    i.physical_from_steps = true;
    i.physical = 12.5;
    let o = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(o.physical, 12.5);
    // Units percent comes from assignments, which are not changed here.
    i.physical_from_steps = false;
    i.percent = PercentKind::Units;
    let o = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(o.physical, 12.5);
    // Completion sets physical to 100.
    i.percent = PercentKind::Physical;
    i.finish = Some(at(1, 17));
    let o = auto_progress(&i, &cal, old, new, RemainingRule::Always).unwrap();
    assert_eq!(o.physical, 100.);
}

#[test]
fn apply_progress_moves_the_data_date_and_respects_scope() {
    let mut p = leveling_project();
    let dd = p.model.project.data_date.unwrap();
    // The same data date is accepted and progresses nothing.
    let mut same = p.clone();
    let n = apply_progress(&mut same, dd, ProgressScope::All, RemainingRule::Always);
    assert_eq!(n, Ok(0));
    assert_eq!(same.model, p.model);
    let mut selected = p.clone();
    let set: BTreeSet<usize> = [1].into();
    let n = apply_progress(
        &mut selected,
        dd + DAY,
        ProgressScope::Selected(&set),
        RemainingRule::Always,
    )
    .unwrap();
    assert_eq!(n, 1);
    assert_eq!(selected.model.activities[1].status, StatusType::InProgress);
    assert_eq!(selected.model.activities[0].status, StatusType::NotStarted);
    assert_eq!(
        selected.details[&110].percent, 50.,
        "physical percent activity"
    );
    // Apply Actuals touches only activities set to compute actuals.
    let mut actuals = p.clone();
    let n = apply_progress(
        &mut actuals,
        dd + DAY,
        ProgressScope::AutoComputeActuals,
        RemainingRule::Always,
    )
    .unwrap();
    assert_eq!(n, 0);
    assert_eq!(actuals.model.project.data_date, Some(dd + DAY));
    records::set_task_value(
        &mut actuals.document,
        120,
        1,
        "auto_compute_act_flag",
        "Y".into(),
    );
    let n = apply_progress(
        &mut actuals,
        dd + 2 * DAY,
        ProgressScope::AutoComputeActuals,
        RemainingRule::Always,
    )
    .unwrap();
    assert_eq!(n, 1);
    assert_eq!(actuals.model.activities[2].status, StatusType::Completed);
    // All activities: the three parallel ones finish within two days.
    let n = apply_progress(
        &mut p,
        dd + 7 * DAY,
        ProgressScope::All,
        RemainingRule::Always,
    )
    .unwrap();
    assert_eq!(n, 3);
    assert!(p
        .model
        .activities
        .iter()
        .all(|a| a.status == StatusType::Completed));
    assert_eq!(p.model.project.data_date, Some(dd + 7 * DAY));
    // The progress survives export and reimport.
    let reread = ProjectData::from_document(
        Document::parse(&p.updated_document().text()).unwrap(),
        1,
        "x".into(),
    )
    .unwrap();
    assert!(reread
        .model
        .activities
        .iter()
        .all(|a| a.status == StatusType::Completed));
    let absolute = |m: &Model| m.project.data_date.map(|d| d + m.origin_serial * DAY);
    assert_eq!(absolute(&reread.model), absolute(&p.model));
    assert_eq!(reread.details[&110].percent, 100.);
}

#[test]
fn update_progress_and_apply_actuals_dialogs_are_one_undo_step() {
    let (ctx, mut app, mut painter, mut time, path) =
        open_app(leveling_project(), "progress-dialog");
    let before = app.project.model.clone();
    let dd = before.project.data_date.unwrap();
    app.open_update_progress();
    frames(&mut app, &ctx, &mut painter, &mut time);
    let origin = app.project.model.origin_serial;
    app.sched.progress.update.as_mut().unwrap().new_date = display_date(origin, Some(dd + DAY));
    painter.click_action(&mut app, &ctx, &mut time, "update_by_type");
    assert_eq!(
        app.sched.progress.update.as_ref().unwrap().rule,
        RemainingRule::DurationType
    );
    let undo = app.history.counts().0;
    painter.click_action(&mut app, &ctx, &mut time, "update_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.sched.progress.update.is_none());
    assert_eq!(app.project.model.project.data_date, Some(dd + DAY));
    assert!(app
        .project
        .model
        .activities
        .iter()
        .all(|a| a.status == StatusType::InProgress));
    assert_eq!(app.history.counts().0, undo + 1, "one undo step");
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.project.model, before);
    assert!(app.history.redo(&mut app.project));
    assert_eq!(app.project.model.project.data_date, Some(dd + DAY));
    // An earlier date is accepted: the data date moves back, the progress
    // already recorded stays, and the dialog closes with its notice.
    let progressed = app.project.model.activities.clone();
    app.open_update_progress();
    frames(&mut app, &ctx, &mut painter, &mut time);
    app.sched.progress.update.as_mut().unwrap().new_date = display_date(origin, Some(dd));
    let undo = app.history.counts().0;
    painter.click_action(&mut app, &ctx, &mut time, "update_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.sched.progress.update.is_none());
    assert!(app.sched.progress.done.is_some());
    assert_eq!(app.project.model.project.data_date, Some(dd));
    assert_eq!(app.project.model.activities, progressed);
    assert_eq!(app.history.counts().0, undo + 1, "one undo step");
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.project.model.project.data_date, Some(dd + DAY));
    app.sched.progress.done = None;
    // Apply Actuals moves the data date and records the apply date.
    app.error = None;
    app.open_apply_actuals();
    frames(&mut app, &ctx, &mut painter, &mut time);
    app.sched.progress.actuals.as_mut().unwrap().new_date =
        display_date(origin, Some(dd + 3 * DAY));
    let undo = app.history.counts().0;
    painter.click_action(&mut app, &ctx, &mut time, "actuals_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.project.model.project.data_date, Some(dd + 3 * DAY));
    assert_eq!(app.history.counts().0, undo + 1);
    let stored = app
        .project
        .document
        .table("PROJECT")
        .map(|t| t.value(&t.rows[0], "apply_actuals_date").to_string())
        .unwrap();
    assert_eq!(stored, date_string(origin, Some(dd + 3 * DAY)));
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.project.model.project.data_date, Some(dd + DAY));
    drop(app);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------- assignment costs

#[test]
fn recalculate_costs_uses_prices_in_effect_and_lists_skips() {
    let mut p = leveling_project();
    // Stored costs use a 40/h price; the resource now costs 50/h.
    let report = recalculate_assignment_costs(&mut p.document, 1, false, false);
    assert_eq!((report.updated, report.unchanged), (3, 0), "{report:?}");
    let t = p.document.table("TASKRSRC").unwrap();
    for r in &t.rows {
        assert_eq!(t.value(r, "cost_per_qty"), "50");
        assert_eq!(t.value(r, "target_cost"), "800");
        assert_eq!(t.value(r, "remain_cost"), "800");
    }
    let again = recalculate_assignment_costs(&mut p.document, 1, false, false);
    assert_eq!((again.updated, again.unchanged), (0, 3));
    // Overtime factor sync and overtime cost.
    let row = 0;
    records::set(&mut p.document, "TASKRSRC", row, "act_ot_qty", "2".into());
    let report = recalculate_assignment_costs(&mut p.document, 1, false, true);
    assert_eq!(report.updated, 3);
    assert_eq!(
        records::get(&p.document, "TASKRSRC", row, "ot_factor"),
        "1.5"
    );
    assert_eq!(
        records::get(&p.document, "TASKRSRC", row, "act_ot_cost"),
        "150"
    );
    // Role and override prices, unlinked costs and price changes are skipped.
    records::set(
        &mut p.document,
        "TASKRSRC",
        0,
        "cost_per_qty_source_type",
        "ST_Role".into(),
    );
    records::set(
        &mut p.document,
        "TASKRSRC",
        1,
        "cost_qty_link_flag",
        "N".into(),
    );
    p.document.table_mut("RSRCRATE").add(&[
        ("rsrc_rate_id", "2".into()),
        ("rsrc_id", "7".into()),
        ("max_qty_per_hr", "1".into()),
        ("cost_per_qty", "60".into()),
        ("start_date", "2026-11-03 00:00".into()),
    ]);
    records::set(
        &mut p.document,
        "TASKRSRC",
        2,
        "target_start_date",
        "2026-11-02 08:00".into(),
    );
    records::set(
        &mut p.document,
        "TASKRSRC",
        2,
        "target_end_date",
        "2026-11-04 17:00".into(),
    );
    let report = recalculate_assignment_costs(&mut p.document, 1, false, false);
    assert_eq!(report.updated, 0);
    assert_eq!(report.skipped.len(), 3, "{:?}", report.skipped);
    // Inactive resources are left out when asked.
    records::set(&mut p.document, "RSRC", 0, "active_flag", "N".into());
    let report = recalculate_assignment_costs(&mut p.document, 1, true, false);
    assert_eq!(report, RecalcReport::default());
}

#[test]
fn recalc_dialog_is_one_undo_step() {
    let (ctx, mut app, mut painter, mut time, path) = open_app(leveling_project(), "recalc-dialog");
    let before = app.project.document.text();
    app.open_recalc_costs();
    frames(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "recalc_run");
    assert!(app.sched.progress.recalc.is_none());
    assert!(
        app.status
            .starts_with("Assignment costs recalculated: 3 updated"),
        "{}",
        app.status
    );
    assert_ne!(app.project.document.text(), before);
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.project.document.text(), before);
    drop(app);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------- renders

/// Software renders of every dialog in this module (ignored `artifacts/`).
#[test]
fn render_progress_dialogs_to_png() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (ctx, mut app, mut painter, mut time, path) =
        open_app(leveling_project(), "progress-render");
    let shots: [(&str, &str, Vec2); 6] = [
        (
            "headless-level-resources.png",
            "level",
            Vec2::new(600., 508.),
        ),
        (
            "headless-select-resources.png",
            "select_rsrc",
            Vec2::new(452., 300.),
        ),
        (
            "headless-update-progress.png",
            "update",
            Vec2::new(590., 277.),
        ),
        (
            "headless-apply-actuals.png",
            "actuals",
            Vec2::new(613., 517.),
        ),
        (
            "headless-store-period.png",
            "store_period",
            Vec2::new(420., 110.),
        ),
        ("headless-recalc-costs.png", "recalc", Vec2::new(480., 190.)),
    ];
    for (name, salt, client) in shots {
        app.sched.progress = Default::default();
        match salt {
            "level" => {
                app.open_level_resources();
                app.sched
                    .progress
                    .level
                    .as_mut()
                    .unwrap()
                    .priorities
                    .push(PriorityRow::new("total_float_hr_cnt", false));
            }
            "select_rsrc" => {
                app.open_level_resources();
                let level = app.sched.progress.level.as_mut().unwrap();
                level.all_resources = false;
                app.sched.progress.select_draft = level.selected.clone();
                app.sched.progress.select_resources = true;
            }
            "update" => app.open_update_progress(),
            "actuals" => app.open_apply_actuals(),
            "store_period" => app.sched.progress.store_period = true,
            _ => app.open_recalc_costs(),
        }
        frames(&mut app, &ctx, &mut painter, &mut time);
        let (image, triangles) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        assert!(triangles > 100, "{name} draws the dialog");
        let close = app.action_rects[&format!("{salt}_title_close")];
        let size = client + Vec2::new(2., 33.);
        let left = (close[2] + 1. - size.x).round().max(0.) as u32;
        let top = (close[1] - 1.).round().max(0.) as u32;
        let cropped =
            image::imageops::crop_imm(&image, left, top, size.x as u32, size.y as u32).to_image();
        cropped.save(artifacts.join(name)).unwrap();
        println!("Software-rendered: {}", artifacts.join(name).display());
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------- leveling options

#[test]
fn level_only_within_float_leaves_an_activity_that_cannot_wait_and_logs_it() {
    // The three parallel activities have no float, so none may be delayed:
    // the first claims the crew, the other two stay and are listed.
    let (ctx, mut app, mut painter, mut time, path) = open_app(leveling_project(), "level-float");
    let before = dates(&app.project.model);
    app.open_level_resources();
    frames(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "level_within_float");
    assert!(app.sched.progress.level.as_ref().unwrap().within_float);
    painter.click_action(&mut app, &ctx, &mut time, "level_run");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(delayed_by_leveling(&app.project.model).is_empty());
    assert_eq!(dates(&app.project.model), before);
    assert_eq!(app.sched.progress.left_over.len(), 2);
    assert!(app
        .log
        .iter()
        .any(|l| { l.contains("Activities left over-allocated") && l.ends_with('2') }));
    assert!(
        app.status.contains("2 left over-allocated"),
        "{}",
        app.status
    );
    assert_eq!(
        sched_value(&app.project.document, 1, "level_within_float_flag").as_deref(),
        Some("Y")
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn preserved_float_is_written_in_hours_and_applies_only_within_float() {
    let mut p = leveling_project();
    let mut s = LevelSettings::read(&p, false);
    s.within_float = true;
    s.float_text = "2d".into();
    assert!(s.write(&mut p).unwrap());
    // Two 8-hour days.
    assert_eq!(
        sched_value(&p.document, 1, "level_float_thrs_cnt").as_deref(),
        Some("16")
    );
    p.refresh_derived_durations();
    assert!(p.resources.within_float);
    assert_eq!(p.resources.preserve_float, 960);
    assert_eq!(LevelSettings::read(&p, false).float_text, "2d");
    // The same text with the mode off is kept but not read by the engine.
    s.within_float = false;
    assert!(s.write(&mut p).unwrap());
    p.refresh_derived_durations();
    assert!(!p.resources.within_float);
    assert_eq!(p.resources.preserve_float, 0);
    assert_eq!(
        sched_value(&p.document, 1, "level_float_thrs_cnt").as_deref(),
        Some("16")
    );
}

#[test]
fn select_resources_edits_the_stored_list_and_round_trips() {
    let (ctx, mut app, mut painter, mut time, path) = open_app(leveling_project(), "level-select");
    app.open_level_resources();
    frames(&mut app, &ctx, &mut painter, &mut time);
    // Level all resources is on: the list button is unavailable and the
    // list starts with every assigned resource.
    assert!(app.sched.progress.level.as_ref().unwrap().all_resources);
    assert_eq!(
        app.sched.progress.level.as_ref().unwrap().selected,
        BTreeSet::from([7])
    );
    painter.click_action(&mut app, &ctx, &mut time, "level_select_resources");
    assert!(!app.sched.progress.select_resources, "disabled while all");
    painter.click_action(&mut app, &ctx, &mut time, "level_all_resources");
    assert!(!app.sched.progress.level.as_ref().unwrap().all_resources);
    // Cancel keeps the list.
    painter.click_action(&mut app, &ctx, &mut time, "level_select_resources");
    assert!(app.sched.progress.select_resources);
    frames(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "select_rsrc_0");
    painter.click_action(&mut app, &ctx, &mut time, "select_rsrc_cancel");
    assert!(!app.sched.progress.select_resources);
    assert_eq!(
        app.sched.progress.level.as_ref().unwrap().selected,
        BTreeSet::from([7])
    );
    // OK takes the edited list.
    painter.click_action(&mut app, &ctx, &mut time, "level_select_resources");
    frames(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "select_rsrc_0");
    painter.click_action(&mut app, &ctx, &mut time, "select_rsrc_ok");
    assert!(app
        .sched
        .progress
        .level
        .as_ref()
        .unwrap()
        .selected
        .is_empty());
    painter.click_action(&mut app, &ctx, &mut time, "level_select_resources");
    frames(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "select_rsrc_0");
    painter.click_action(&mut app, &ctx, &mut time, "select_rsrc_ok");
    assert_eq!(
        app.sched.progress.level.as_ref().unwrap().selected,
        BTreeSet::from([7])
    );
    painter.click_action(&mut app, &ctx, &mut time, "level_close");
    assert!(app.sched.progress.level.is_none());
    assert_eq!(
        sched_value(&app.project.document, 1, "level_all_rsrc_flag").as_deref(),
        Some("N")
    );
    // The list went to RSRCLEVELLIST and survives an XER round trip.
    let list = app.project.document.table("RSRCLEVELLIST").unwrap();
    assert_eq!(list.rows.len(), 1);
    assert_eq!(list.value(&list.rows[0], "rsrc_id"), "7");
    let schedoptions = sched_value(&app.project.document, 1, "schedoptions_id").unwrap();
    assert_eq!(list.value(&list.rows[0], "schedoptions_id"), schedoptions);
    let reread = ProjectData::from_document(
        Document::parse(&app.project.updated_document().text()).unwrap(),
        1,
        "x".into(),
    )
    .unwrap();
    assert_eq!(reread.resources.selected, Some(BTreeSet::from([7])));
    assert_eq!(
        selected_level_resources(&reread),
        BTreeSet::from([7]),
        "the dialog reads the list back"
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn level_all_resources_levels_like_selecting_every_resource() {
    let all = leveling_project();
    let expected = engine_leveled(&all);
    let mut chosen = all.clone();
    let mut s = LevelSettings::read(&chosen, false);
    s.all_resources = false;
    s.selected = BTreeSet::from([7]);
    assert!(s.write(&mut chosen).unwrap());
    assert_eq!(dates(&engine_leveled(&chosen)), dates(&expected));
    // An empty list levels nothing.
    s.selected.clear();
    assert!(s.write(&mut chosen).unwrap());
    assert!(chosen
        .document
        .table("RSRCLEVELLIST")
        .unwrap()
        .rows
        .is_empty());
    assert!(delayed_by_leveling(&engine_leveled(&chosen)).is_empty());
    // With every resource leveled again the stored rows are left alone.
    s.all_resources = true;
    assert!(s.write(&mut chosen).unwrap());
    assert!(!s.write(&mut chosen).unwrap());
}

#[test]
fn recalculating_costs_after_leveling_equals_recalculating_afterwards() {
    let run = |recalc: bool| {
        let (ctx, mut app, mut painter, mut time, path) =
            open_app(leveling_project(), "level-recalc");
        app.open_level_resources();
        frames(&mut app, &ctx, &mut painter, &mut time);
        if recalc {
            painter.click_action(&mut app, &ctx, &mut time, "level_recalc_costs");
        }
        painter.click_action(&mut app, &ctx, &mut time, "level_run");
        assert!(app.error.is_none(), "{:?}", app.error);
        let doc = app.project.document.clone();
        let preference = app.recalc_costs_after_leveling();
        drop(app);
        let _ = std::fs::remove_file(path);
        (doc, preference)
    };
    let (with, stored) = run(true);
    assert!(stored, "the check box is remembered");
    let (mut without, stored) = run(false);
    assert!(!stored);
    let costs = |d: &Document| {
        let t = d.table("TASKRSRC").unwrap();
        t.rows
            .iter()
            .map(|r| {
                ["cost_per_qty", "target_cost", "remain_cost"].map(|k| t.value(r, k).to_string())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(costs(&without)[0], ["40", "640", "640"]);
    recalculate_assignment_costs(&mut without, 1, false, false);
    assert_eq!(costs(&with)[0], ["50", "800", "800"]);
    assert_eq!(
        with.table("TASKRSRC").unwrap().rows,
        without.table("TASKRSRC").unwrap().rows
    );
}

#[test]
fn rebuilt_leveling_inputs_follow_the_stored_selection_and_float_options() {
    // A starter project has no SCHEDOPTIONS row: the options a new row
    // carries level everything and ignore float.
    let mut starter = openxer_xer::starter();
    let mut s = LevelSettings::read(&starter, false);
    s.within_float = true;
    s.float_text = "1d".into();
    s.priorities.push(PriorityRow::new("priority_type", true));
    s.write(&mut starter).unwrap();
    let reread = ProjectData::from_document(
        Document::parse(&starter.updated_document().text()).unwrap(),
        1,
        "x".into(),
    )
    .unwrap();
    assert!(reread.resources.within_float);
    assert!(reread.resources.preserve_float > 0);
    assert_eq!(reread.resources.selected, None);
}

/// Update Progress with "selected activities" progresses every selected
/// activity of a multi-selection, and nothing else.
#[test]
fn update_progress_selected_uses_the_whole_selection() {
    let (ctx, mut app, mut painter, mut time, path) =
        open_app(leveling_project(), "progress-multi");
    let dd = app.project.model.project.data_date.unwrap();
    let n = app.project.model.activities.len();
    assert!(n >= 3, "{n}");
    app.select_activities(&[0, 1]);
    assert_eq!(app.selected_in_order().len(), 2);
    app.open_update_progress();
    frames(&mut app, &ctx, &mut painter, &mut time);
    let origin = app.project.model.origin_serial;
    let draft = app.sched.progress.update.as_mut().unwrap();
    draft.new_date = display_date(origin, Some(dd + DAY));
    draft.selected_only = true;
    painter.click_action(&mut app, &ctx, &mut time, "update_apply");
    assert!(app.error.is_none(), "{:?}", app.error);
    let status = |i: usize| app.project.model.activities[i].status;
    assert_eq!(status(0), StatusType::InProgress);
    assert_eq!(
        status(1),
        StatusType::InProgress,
        "the second selected one too"
    );
    for i in 2..n {
        assert_eq!(status(i), StatusType::NotStarted, "{i} was not selected");
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Tools > Recalculate Assignment Costs works on every open project, as its
/// "current project(s)" wording says, as one undo step across the set.
#[test]
fn recalculate_assignment_costs_covers_every_open_project() {
    let path = temp_db("recalc-all");
    let mut store = Store::open(path.clone()).unwrap();
    let mut first = leveling_project();
    first.source_name = "first.xer".into();
    let mut second = leveling_project();
    second.source_name = "second.xer".into();
    let second_id = store.save(&second).unwrap();
    let first_id = store.save(&first).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, first);
    app.restore_open_set();
    app.open_project_set(&[first_id, second_id]);
    assert!(app.open_set.multi());
    let costs = |p: &ProjectData| -> Vec<String> {
        let t = p.document.table("TASKRSRC").unwrap();
        t.rows
            .iter()
            .map(|r| t.value(r, "target_cost").to_string())
            .collect()
    };
    let before_other = app.open_set.parked[0].project.document.text();
    let other_steps = app.open_set.parked[0].history.counts().0;
    app.open_recalc_costs();
    app.run_recalc_costs(1.);
    assert!(app.status.contains("in 2 projects"), "{}", app.status);
    assert!(
        app.status.contains(": 6 updated"),
        "3 in each project: {}",
        app.status
    );
    let other = &app.open_set.parked[0];
    assert_ne!(
        other.project.document.text(),
        before_other,
        "the other open project is recalculated too"
    );
    assert_eq!(costs(&other.project), costs(&app.project));
    assert_eq!(other.history.counts().0, other_steps + 1);
    drop(app);
    let _ = std::fs::remove_file(path);
}

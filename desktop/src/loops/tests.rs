use super::analysis::*;
use super::*;
use openxer_engine::model::{Activity, Link, ProjectSettings};
use openxer_engine::options::ScheduleOptions;

fn act(id: i64, kind: TaskType) -> Activity {
    Activity {
        task_id: id,
        task_code: format!("A{id}"),
        task_type: kind,
        rem: 480,
        ..Activity::default()
    }
}

fn net(kinds: &[TaskType], links: &[(usize, usize, LinkType, i64)]) -> Model {
    let mut m = Model {
        origin_serial: 0,
        project_calendar: 0,
        calendars: vec![],
        activities: kinds
            .iter()
            .enumerate()
            .map(|(i, &k)| act(i as i64 + 1, k))
            .collect(),
        links: links
            .iter()
            .map(|&(pred, succ, link_type, lag)| Link {
                pred,
                succ,
                link_type,
                lag,
                driving_path: false,
            })
            .collect(),
        project: ProjectSettings::default(),
        options: ScheduleOptions::default(),
    };
    m.rebuild_links().unwrap();
    m
}

fn run(m: &Model, hints: &Hints) -> LoopReport {
    analyze(m, hints, &|i| m.activities[i].task_code.clone())
}

use LinkType::{FF, FS, SS};
use TaskType::{Loe, Mile, Task};

/// S(0) -> T(1) -> M(2) -> F(3), S -> W(4) SS, F -> W FF, and the reversed
/// start tie W -> S SS.
pub(super) fn window_net() -> Model {
    net(
        &[Mile, Task, Task, Mile, Loe],
        &[
            (0, 1, FS, 0),
            (1, 2, FS, 0),
            (2, 3, FS, 0),
            (3, 4, FF, 0),
            (4, 0, SS, 0),
        ],
    )
}

#[test]
fn no_loop_gives_an_empty_report() {
    let m = net(&[Task, Task, Task], &[(0, 1, FS, 0), (1, 2, FS, 0)]);
    assert!(!has_loop(&m));
    let r = run(&m, &Hints::default());
    assert!(r.is_empty() && r.plan.is_empty());
}

#[test]
fn reversed_level_of_effort_tie_is_suggested_for_reversal() {
    let m = window_net();
    assert!(has_loop(&m));
    let r = run(&m, &Hints::default());
    assert_eq!(r.groups.len(), 1);
    let g = &r.groups[0];
    assert_eq!(g.members, vec![0, 1, 2, 3, 4]);
    assert_eq!(g.chain.len(), 5);
    let s = g.suggestion.as_ref().unwrap();
    assert_eq!(m.links[s.link].pred, 4);
    assert_eq!(m.links[s.link].succ, 0);
    assert!(s.reversible);
    assert!(s.reason.contains("probably reversed"), "{}", s.reason);
    assert!(s.reason.contains("Level of Effort"));
    assert_eq!(r.plan.len(), 1);
    // The chain reads in relationship order.
    for pair in g.chain.windows(2) {
        assert_eq!(pair[0].succ, pair[1].pred);
    }
    assert_eq!(g.chain[4].succ, g.chain[0].pred);
}

#[test]
fn two_independent_loops_are_two_groups() {
    let m = net(
        &[Task; 5],
        &[
            (0, 1, FS, 0),
            (1, 0, FS, 0),
            (2, 3, FS, 0),
            (3, 4, FS, 0),
            (4, 2, FS, 0),
        ],
    );
    let r = run(&m, &Hints::default());
    assert_eq!(r.groups.len(), 2);
    assert_eq!(r.groups[0].members.len(), 3, "largest first");
    assert_eq!(r.groups[1].members.len(), 2);
    assert_eq!(r.plan.len(), 2);
}

#[test]
fn a_shared_tie_clears_nested_loops_with_one_removal() {
    // 0 -> 1 -> 2 -> 0 and 0 -> 1 -> 3 -> 0 share 0 -> 1 and 3/2 -> 0 differ.
    // Removing 0 -> 1 clears both; removing any other tie leaves one loop.
    let m = net(
        &[Task; 4],
        &[
            (0, 1, FS, 0),
            (1, 2, FS, 0),
            (2, 0, FS, 0),
            (1, 3, FS, 0),
            (3, 0, FS, 0),
        ],
    );
    let r = run(&m, &Hints::default());
    assert_eq!(r.groups.len(), 1);
    let s = r.groups[0].suggestion.as_ref().unwrap();
    assert!(s.link == 0, "the shared tie, got {}", s.link);
    assert_eq!(r.plan.len(), 1);
    assert!(s.reason.contains("clears this loop group"));
}

#[test]
fn a_self_tie_is_a_loop_of_one() {
    let mut m = net(&[Task, Task], &[(0, 1, FS, 0)]);
    m.links.push(Link {
        pred: 1,
        succ: 1,
        link_type: FS,
        lag: 0,
        driving_path: false,
    });
    let r = run(&m, &Hints::default());
    assert_eq!(r.groups.len(), 1);
    assert_eq!(r.groups[0].chain.len(), 1);
    assert_eq!(r.groups[0].members, vec![1]);
    assert_eq!(r.plan.len(), 1);
}

#[test]
fn a_tie_pointing_backwards_in_time_is_suggested() {
    let mut m = net(&[Task; 3], &[(0, 1, FS, 0), (1, 2, FS, 0), (2, 0, FS, 0)]);
    let day = openxer_engine::calendar::DAY;
    // The last good schedule ran 0, 1, 2 in order; 2 -> 0 points back.
    for (i, a) in m.activities.iter_mut().enumerate() {
        a.early_start = Some(i as i64 * 10 * day);
        a.early_finish = Some(i as i64 * 10 * day + 5 * day);
    }
    let r = run(&m, &Hints::default());
    let s = r.groups[0].suggestion.as_ref().unwrap();
    assert_eq!(s.link, 2);
    assert!(s.reason.contains("backwards in time"), "{}", s.reason);
}

#[test]
fn the_tie_added_last_is_suggested_when_nothing_else_stands_out() {
    let m = net(&[Task; 3], &[(0, 1, FS, 0), (1, 2, FS, 0), (2, 0, FS, 0)]);
    let mut hints = Hints::default();
    hints.added.insert(link_key(&m, &m.links[1]), 3);
    hints.added.insert(link_key(&m, &m.links[0]), 1);
    let r = run(&m, &hints);
    let s = r.groups[0].suggestion.as_ref().unwrap();
    assert_eq!(s.link, 1);
    assert!(s.reason.contains("added most recently"), "{}", s.reason);
    // The update date picks the newest of the stored stamps.
    let mut hints = Hints::default();
    hints
        .updated
        .insert(link_key(&m, &m.links[2]), "2026-05-01 10:00".into());
    hints
        .updated
        .insert(link_key(&m, &m.links[0]), "2026-01-01 10:00".into());
    let r = run(&m, &hints);
    assert_eq!(r.groups[0].suggestion.as_ref().unwrap().link, 2);
}

#[test]
fn big_networks_analyze_quickly() {
    // 5,000 activities in a long ladder with cross ties, one big loop group.
    let n = 5000;
    let kinds = vec![Task; n];
    let mut links = vec![];
    for i in 0..n - 1 {
        links.push((i, i + 1, FS, 0));
        if i + 7 < n {
            links.push((i, i + 7, SS, 0));
        }
    }
    links.push((n - 1, 0, FS, 0));
    let m = net(&kinds, &links);
    let started = std::time::Instant::now();
    let r = run(&m, &Hints::default());
    let took = started.elapsed();
    assert_eq!(r.groups.len(), 1);
    assert_eq!(r.groups[0].members.len(), n);
    // Well under a second in a release build; a debug build on a busy
    // machine gets more room.
    let budget = if cfg!(debug_assertions) { 6.0 } else { 1.0 };
    assert!(took.as_secs_f64() < budget, "took {took:?}");
    assert!(!r.plan.is_empty());
}

// ------------------------------------------------------------ dialog by clicks

use crate::headless_ui::{click, frame, key};

/// The starter project with its first five activities wired as the work
/// window case: the start tie W -> S is reversed and closes the loop.
fn looped_project() -> ProjectData {
    let mut project = openxer_xer::starter();
    let looped = window_net();
    let m = &mut project.model;
    m.activities[0].task_type = Mile;
    m.activities[3].task_type = Mile;
    m.activities[4].task_type = Loe;
    m.links = looped.links.clone();
    m.rebuild_links().unwrap();
    project
}

fn looped_app(name: &str) -> (std::path::PathBuf, egui::Context, App, f64) {
    let path = std::env::temp_dir().join(format!(
        "openxer-loops-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, looped_project());
    let mut time = 0.;
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    (path, ctx, app, time)
}

fn settle(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    for _ in 0..3 {
        let _ = frame(app, ctx, vec![], time);
    }
}

fn pairs(m: &Model) -> Vec<(usize, usize, LinkType)> {
    let mut v: Vec<_> = m
        .links
        .iter()
        .map(|l| (l.pred, l.succ, l.link_type))
        .collect();
    v.sort_by_key(|&(p, s, t)| (p, s, t as u8));
    v
}

#[test]
fn f9_on_a_looped_project_opens_the_dialog_and_delete_is_one_undo_step() {
    let (path, ctx, mut app, mut time) = looped_app("delete");
    let before = pairs(&app.project.model);
    app.open_schedule_dialog();
    settle(&mut app, &ctx, &mut time);
    click(&mut app, &ctx, "schedule_run", &mut time);
    // The run waits a frame behind its wait cursor, so Circular
    // Relationships first appears in the frame after the click; a window's
    // first frame only measures it, and a press is matched against what
    // the frame before drew. Let it draw before using it, as a user does.
    settle(&mut app, &ctx, &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(
        app.loops.open,
        "F9 opens Circular Relationships; status {:?} log {:?}",
        app.status, app.log
    );
    assert!(!app.sched.schedule_open);
    assert_eq!(app.schedule_runs, 0);
    assert_eq!(app.loops.report.groups.len(), 1);
    // The reversed tie is selected from the start, and Reverse is enabled.
    let tie = app.loops.report.groups[0].chain[app.loops.row];
    assert_eq!((tie.pred, tie.succ), (4, 0));
    let steps = app.history.steps();
    click(&mut app, &ctx, "loops_delete", &mut time);
    assert_eq!(app.project.model.links.len(), before.len() - 1);
    assert!(app.loops.report.is_empty());
    assert!(!app.has_loops());
    assert_eq!(app.history.steps(), steps + 1, "one undo step");
    // The dialog says so and offers Schedule.
    click(&mut app, &ctx, "loops_schedule", &mut time);
    assert!(!app.loops.open);
    assert!(app.sched.schedule_open);
    key(&mut app, &ctx, egui::Key::Escape, &mut time);
    // Undo restores the loop; the dialog finds it again when asked.
    app.history_move(&ctx, false);
    assert_eq!(pairs(&app.project.model), before);
    assert!(app.has_loops());
    app.open_loops();
    settle(&mut app, &ctx, &mut time);
    assert_eq!(app.loops.report.groups.len(), 1);
    key(&mut app, &ctx, egui::Key::Escape, &mut time);
    assert!(!app.loops.open, "Escape closes it");
    assert_eq!(pairs(&app.project.model), before, "Close changes nothing");
    let _ = std::fs::remove_file(path);
}

#[test]
fn reverse_turns_the_window_start_tie_round() {
    let (path, ctx, mut app, mut time) = looped_app("reverse");
    app.open_loops();
    settle(&mut app, &ctx, &mut time);
    let steps = app.history.steps();
    click(&mut app, &ctx, "loops_reverse", &mut time);
    let m = &app.project.model;
    assert!(!app.has_loops());
    assert!(
        m.links
            .iter()
            .any(|l| (l.pred, l.succ, l.link_type) == (0, 4, SS)),
        "S -> W (SS) now exists"
    );
    assert!(!m.links.iter().any(|l| (l.pred, l.succ) == (4, 0)));
    assert_eq!(app.history.steps(), steps + 1, "one undo step");
    app.history_move(&ctx, false);
    assert!(app.has_loops(), "undo restores the reversed tie");
    let _ = std::fs::remove_file(path);
}

#[test]
fn show_loop_filters_the_table_and_clears_on_toggle_and_close() {
    let (path, ctx, mut app, mut time) = looped_app("show");
    let all = app.rows().len();
    app.open_loops();
    settle(&mut app, &ctx, &mut time);
    click(&mut app, &ctx, "loops_show", &mut time);
    assert!(app.loops.view.is_some());
    let rows = app.rows();
    assert_eq!(rows.len(), 5);
    assert!(rows.len() < all);
    let view = app.loops.view.clone().unwrap();
    assert!(view.links.contains(&(4, 0)));
    click(&mut app, &ctx, "loops_show", &mut time);
    assert!(app.loops.view.is_none());
    assert_eq!(app.rows().len(), all, "the previous rows are back");
    click(&mut app, &ctx, "loops_show", &mut time);
    click(&mut app, &ctx, "loops_close", &mut time);
    assert!(!app.loops.open && app.loops.view.is_none());
    assert_eq!(app.rows().len(), all);
    let _ = std::fs::remove_file(path);
}

#[test]
fn go_to_selects_the_activity() {
    let (path, ctx, mut app, mut time) = looped_app("goto");
    app.open_loops();
    settle(&mut app, &ctx, &mut time);
    app.selected = Some(9);
    click(&mut app, &ctx, "loops_goto", &mut time);
    let tie = app.loops.report.groups[0].chain[app.loops.row];
    assert_eq!(app.selected, Some(tie.pred));
    assert!(app.scroll_to_selected || app.selected.is_some());
    // Picking another row and going there selects that row's activity.
    let other = (app.loops.row + 1) % 5;
    click(
        &mut app,
        &ctx,
        &format!("loops_chain_row_{other}"),
        &mut time,
    );
    assert_eq!(app.loops.row, other);
    click(&mut app, &ctx, "loops_goto", &mut time);
    let tie = app.loops.report.groups[0].chain[other];
    assert!(app.selected == Some(tie.pred) || app.selected == Some(tie.succ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn level_and_check_schedule_open_the_dialog_too() {
    let (path, ctx, mut app, mut time) = looped_app("others");
    app.sched.progress.manual_level = false;
    app.open_check_schedule();
    settle(&mut app, &ctx, &mut time);
    click(&mut app, &ctx, "check_run", &mut time);
    assert!(app.loops.open, "Check Schedule lists and opens the loops");
    assert!(!app.loops.check_lines.is_empty());
    assert!(app.loops.check_lines[0].starts_with("Loop 1: 5 activities, 5 affected"));
    assert!(app.sched.check_results.is_some());
    app.loops.open = false;
    app.schedule();
    assert!(app.loops.open, "the schedule command opens it as well");
    assert!(app.error.is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn opening_a_looped_project_offers_the_dialog_once() {
    let (path, ctx, mut app, mut time) = looped_app("prompt");
    app.load(looped_project());
    assert!(app.loops.prompt);
    assert!(app.status.contains("circular relationship"));
    settle(&mut app, &ctx, &mut time);
    assert!(app.action_rects.contains_key("loops_prompt_show"));
    click(&mut app, &ctx, "loops_prompt_dismiss", &mut time);
    assert!(!app.loops.prompt && !app.loops.open);
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    assert!(!app.loops.prompt, "not offered again until the next open");
    app.load(looped_project());
    assert!(app.loops.prompt);
    settle(&mut app, &ctx, &mut time);
    click(&mut app, &ctx, "loops_prompt_show", &mut time);
    assert!(app.loops.open && !app.loops.prompt);
    // A project without loops stays quiet.
    app.loops.open = false;
    app.load(openxer_xer::starter());
    assert!(!app.loops.prompt && !app.loops.open);
    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------- renders

/// Renders for a look: `artifacts/loops/*.png` (ignored by git). Software
/// painter only; no native window.
#[test]
fn render_the_circular_relationships_dialog() {
    use crate::headless_render::SoftwarePainter;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/loops");
    std::fs::create_dir_all(&out).unwrap();
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let names = [
        "Start of work window",
        "Install pipe supports",
        "Pressure test line 12",
        "Finish of work window",
        "Work window",
    ];
    let m = &mut project.model;
    m.activities[0].task_type = Mile;
    m.activities[3].task_type = Mile;
    m.activities[4].task_type = Loe;
    m.links = window_net().links;
    m.rebuild_links().unwrap();
    for (i, name) in names.iter().enumerate() {
        let id = project.model.activities[i].task_id;
        project.details.get_mut(&id).unwrap().name = (*name).into();
    }
    let path = std::env::temp_dir().join(format!("openxer-loops-render-{}.db", std::process::id()));
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, Store::open(path.clone()).unwrap(), project);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    let mut shoot = |app: &mut App, name: &str| {
        for _ in 0..4 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        image.save(out.join(format!("{name}.png"))).unwrap();
    };
    app.schedule();
    shoot(&mut app, "loops-dialog");
    app.toggle_loop_view();
    shoot(&mut app, "loops-show-loop");
    app.toggle_loop_view();
    app.reverse_loop_tie();
    shoot(&mut app, "loops-cleared");
    let _ = std::fs::remove_file(path);
}

#[test]
fn many_loops_in_a_big_network_stay_bounded() {
    let n = 10_000;
    let mut seed = 12345u64;
    let mut next = move |m: usize| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as usize) % m
    };
    let mut links = vec![];
    for _ in 0..50_000 {
        let a = next(n - 60);
        links.push((a, a + 1 + next(50), FS, 0));
    }
    for _ in 0..60 {
        let a = 60 + next(n - 60);
        links.push((a, a - 1 - next(59), FS, 0));
    }
    let m = net(&vec![Task; n], &links);
    let started = std::time::Instant::now();
    let r = run(&m, &Hints::default());
    let took = started.elapsed();
    assert!(took.as_secs_f64() < if cfg!(debug_assertions) { 6.0 } else { 1.0 });
    assert!(!r.is_empty());
    assert!(r.plan.len() <= 50);
}

use super::*;
use crate::grid::Row;
use crate::qa_sweep::Driver;

/// A corpus project scheduled by the engine (skipped when the corpus is absent).
fn corpus(name: &str) -> Option<ProjectData> {
    let path = crate::test_corpus::file(&format!("{name}.xer"));
    let doc = Document::read(&path).ok()?;
    let pid = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc, pid, name.into()).ok()?;
    p.model.rebuild_links().ok()?;
    scheduler::schedule(&mut p.model).ok()?;
    Some(p)
}

fn table_activities(d: &Driver) -> BTreeSet<usize> {
    d.app
        .rows()
        .into_iter()
        .filter_map(|r| match r {
            Row::Activity(i) => Some(i),
            Row::Group(_) => None,
        })
        .collect()
}

fn open_submenu(d: &mut Driver) {
    d.click("menu_bar_view");
    assert!(d.hover_text("Activity Critical Path"), "submenu is drawn");
}

fn menu(d: &mut Driver, action: &str) {
    open_submenu(d);
    d.click(action);
}

/// Float Path 1 for `end` as the scheduler numbers it.
fn float_path(p: &ProjectData, end: usize, path: i64, count: usize) -> BTreeSet<usize> {
    let mut m = p.model.clone();
    m.options.multiple_float_paths = true;
    m.options.float_paths_end_activity = Some(m.activities[end].task_id);
    m.options.max_float_paths = count;
    scheduler::schedule(&mut m).unwrap();
    (0..m.activities.len())
        .filter(|&i| m.activities[i].float_path == path)
        .collect()
}

#[test]
fn corpus_backward_path_is_float_path_one_and_more_fewer_and_off_restore() {
    let Some(p) = corpus("6031") else {
        println!("SKIPPED: corpus not found");
        return;
    };
    // The activity with the most ancestors: the project's last finish.
    let end = (0..p.model.activities.len())
        .filter(|&i| {
            let a = &p.model.activities[i];
            !a.preds.is_empty()
                && a.early_finish.is_some()
                && !scheduler::Scheduler::skip_type(a.task_type)
        })
        .max_by_key(|&i| p.model.activities[i].early_finish)
        .unwrap();
    let path1 = float_path(&p, end, 1, 1);
    assert!(path1.len() > 1, "a path of several activities");
    let mut d = Driver::new("acp", p);
    let before = d.app.rows();
    let layout = d.app.grid.layout.clone();
    d.app.selected = Some(end);
    menu(&mut d, "menu_acp_backward");
    assert_eq!(d.app.critical_path.count, 1);
    assert_eq!(table_activities(&d), path1, "the Float Path 1 activities");
    // More paths add the second path's activities; fewer go back.
    menu(&mut d, "menu_acp_more");
    assert_eq!(d.app.critical_path.count, 2);
    let path2 = float_path(&d.app.project, end, 2, 2);
    let two: BTreeSet<usize> = path1.union(&path2).copied().collect();
    assert_eq!(table_activities(&d), two);
    menu(&mut d, "menu_acp_fewer");
    assert_eq!(table_activities(&d), path1);
    // Grouped: one band per path, activities in path order.
    menu(&mut d, "menu_acp_group");
    let rows = d.app.rows();
    assert!(matches!(rows[0], Row::Group(_)));
    assert_eq!(d.app.grid.bands.borrow()[0].label, "Backward Path 1");
    let order: Vec<i64> = rows
        .iter()
        .filter_map(|r| match r {
            Row::Activity(i) => Some(*i),
            _ => None,
        })
        .map(|i| {
            let mut m = d.app.project.model.clone();
            m.options.multiple_float_paths = true;
            m.options.float_paths_end_activity = Some(m.activities[end].task_id);
            m.options.max_float_paths = 1;
            scheduler::schedule(&mut m).unwrap();
            m.activities[i].float_path_order
        })
        .collect();
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "path order {order:?}"
    );
    // Off: the layout's rows come back and the layout itself never changed.
    menu(&mut d, "menu_acp_backward");
    assert!(d.app.critical_path.shown.is_none());
    assert_eq!(d.app.rows(), before);
    assert!(d.app.grid.layout == layout, "the saved layout is untouched");
    assert_eq!(d.app.history.counts().0, 0, "no undo step");
}

#[test]
fn forward_and_both_directions_and_select_paths_dialog() {
    let mut d = Driver::new(
        "acp-starter",
        crate::reports::test_support::scheduled_starter(),
    );
    let model = d.app.project.model.clone();
    let seed = (0..model.activities.len())
        .find(|&i| !model.activities[i].succs.is_empty())
        .unwrap();
    d.app.selected = Some(seed);
    let all = table_activities(&d);
    menu(&mut d, "menu_acp_forward");
    let forward = table_activities(&d);
    assert!(forward.contains(&seed));
    assert!(forward.len() < all.len() || model.activities[seed].preds.is_empty());
    // Both directions: the backward and forward paths together.
    menu(&mut d, "menu_acp_show");
    assert_eq!(
        d.app.critical_path.shown.as_ref().unwrap().direction,
        Direction::Both
    );
    let both = table_activities(&d);
    assert!(both.is_superset(&forward));
    let labels: Vec<String> = d
        .app
        .critical_paths()
        .into_iter()
        .map(|p| p.label)
        .collect();
    assert_eq!(labels[0], "Backward Path 1");
    assert!(labels.iter().any(|l| l == "Forward Path 1"));
    // Select Paths to Show: Cancel and Escape keep every path; unchecking
    // one and OK hides its activities.
    menu(&mut d, "menu_acp_select");
    assert!(d.app.critical_path.select.is_some());
    d.click("acp_select_cancel");
    assert!(d.app.critical_path.select.is_none());
    menu(&mut d, "menu_acp_select");
    d.key(Key::Escape);
    assert!(d.app.critical_path.select.is_none());
    assert_eq!(table_activities(&d), both);
    menu(&mut d, "menu_acp_select");
    d.click("acp_select_check_0");
    d.click("acp_select_ok");
    assert_eq!(
        d.app.critical_path.shown.as_ref().unwrap().hidden,
        BTreeSet::from([0])
    );
    assert!(table_activities(&d).len() <= both.len());
    // The toolbar is off until View > Toolbars turns it on; its Show button
    // turns the paths off again.
    assert!(!d.has("tool_acp_show"));
    d.app
        .views
        .shown_toolbars
        .insert("Activity Critical Path".into());
    d.frames(2);
    assert!(d.has("tool_acp_more"));
    d.click("tool_acp_show");
    assert!(d.app.critical_path.shown.is_none());
    assert_eq!(table_activities(&d), all);
}

#[test]
fn nothing_selected_shows_nothing_and_counts_stay_in_range() {
    let mut d = Driver::new(
        "acp-none",
        crate::reports::test_support::scheduled_starter(),
    );
    d.app.selected = None;
    d.app.toggle_critical_paths(Direction::Both);
    assert!(d.app.critical_path.shown.is_none());
    d.app.change_path_count(false);
    assert_eq!(d.app.critical_path.count, 1);
    for _ in 0..MAX_PATHS + 3 {
        d.app.change_path_count(true);
    }
    assert_eq!(d.app.critical_path.count, MAX_PATHS);
}

fn filter_bar(d: &mut Driver) -> String {
    d.frames(2);
    d.texts()
        .into_iter()
        .map(|(t, _)| t)
        .find(|t| t.starts_with("Filter: "))
        .expect("the layout bar names a filter")
}

#[test]
fn the_layout_bar_names_the_path_filter_while_critical_paths_show() {
    let mut d = Driver::new("acp-bar", crate::reports::test_support::scheduled_starter());
    d.home();
    let seed = (0..d.app.project.model.activities.len())
        .find(|&i| !d.app.project.model.activities[i].succs.is_empty())
        .unwrap();
    d.app.selected = Some(seed);
    let before = filter_bar(&mut d);
    assert_eq!(before, "Filter: All Activities");
    for (action, name) in [
        ("menu_acp_backward", "Backward Activity Critical Paths"),
        ("menu_acp_forward", "Forward Activity Critical Paths"),
        ("menu_acp_show", "Activity Critical Paths"),
    ] {
        menu(&mut d, action);
        assert_eq!(filter_bar(&mut d), format!("Filter: {name}"));
        // The layout's own filter is untouched underneath.
        assert_eq!(d.app.current_filter_name(), "All Activities");
    }
    // Turning the paths off restores the layout's filter wording.
    menu(&mut d, "menu_acp_show");
    assert!(d.app.critical_path.shown.is_none());
    assert_eq!(filter_bar(&mut d), before);
    // A layout filter in force is named again after the paths go.
    d.app
        .apply_check_filter(crate::layout_opts::checks::CheckFilter::NegativeFloat);
    let filtered = filter_bar(&mut d);
    assert_ne!(filtered, before);
    menu(&mut d, "menu_acp_backward");
    assert_eq!(
        filter_bar(&mut d),
        "Filter: Backward Activity Critical Paths"
    );
    menu(&mut d, "menu_acp_backward");
    assert_eq!(filter_bar(&mut d), filtered);
}

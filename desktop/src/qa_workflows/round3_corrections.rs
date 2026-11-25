//! Workflow: what the reference was observed to do in the third round,
//! where OpenXER had guessed.
//!
//! - Projects window: several rows can be selected; Fill Down is enabled
//!   on the Project Name column and greyed on Project ID, and copies the
//!   name through the edit layer as one undo step.
//! - Resource Assignments window: the Delete key asks first (No is the
//!   default) and Yes deletes the assignment, one undo step.
//! - Edit Relationship: Delete asks first (Yes is the default).
//! - Gantt chart: a bar and a milestone open the one chart menu; a
//!   right-click on a relationship line selects it and opens the same menu,
//!   and Edit Relationship opens when that menu is closed with Escape.
//!
//! Each change is checked, then one Undo puts the project back.
use super::*;

/// The project rows of the Projects table that show `text`.
fn project_row_with(d: &mut Driver, text: &str) -> usize {
    let texts = d.texts();
    (0..)
        .take_while(|i| d.has(&format!("projects_row_{i}")))
        .find(|i| {
            let r = d.rect(&format!("projects_row_{i}"));
            texts
                .iter()
                .any(|(t, at)| t == text && r.contains(at.center()))
        })
        .unwrap_or_else(|| panic!("a Projects row shows {text:?}"))
}

/// Click the Projects table cell of row `row`, field `field`.
fn project_cell(d: &mut Driver, row: usize, field: usize, modifiers: egui::Modifiers) {
    d.frames(1);
    let x = d.rect(&format!("projects_col_{field}")).center().x;
    let y = d.rect(&format!("projects_row_{row}")).center().y;
    d.press_at(Pos2::new(x, y), egui::PointerButton::Primary, modifiers);
    d.frames(1);
}

/// Whether Edit > Fill Down is enabled now.
fn fill_down_enabled(d: &mut Driver) -> bool {
    d.frames(1);
    d.app.view_fill_ready()
}

#[test]
fn workflow_projects_fill_down_copies_the_project_name() {
    let mut d = fresh("wf-r3-projects-fill");
    let first = views::wbs::project_name(&d.app.project);
    new_project(&mut d, "PMP2", "Pump Two", "01-Dec-26", "31-Dec-26");
    let steps = d.app.history.counts().0;
    show(&mut d, "projects");
    // Click the other project's name first, then Ctrl+click the current
    // project's: Fill Down copies the first name to the current project.
    let source = project_row_with(&mut d, &first);
    let target = project_row_with(&mut d, "Pump Two");
    project_cell(&mut d, source, 0, Default::default());
    project_cell(&mut d, target, 0, ctrl());
    assert!(
        !fill_down_enabled(&mut d),
        "Fill Down is greyed on the Project ID column, as observed"
    );
    project_cell(&mut d, source, 1, Default::default());
    project_cell(&mut d, target, 1, ctrl());
    assert!(
        fill_down_enabled(&mut d),
        "Fill Down is enabled on Project Name with two rows, as observed"
    );
    d.click("menu_bar_edit");
    d.click("edit_fill_down");
    d.frames(2);
    d.assert_clean("projects fill down");
    assert_eq!(
        views::wbs::project_name(&d.app.project),
        first,
        "the current project takes the first row's name: {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(views::wbs::project_name(&d.app.project), "Pump Two");
}

fn assignment_rows(d: &Driver) -> usize {
    d.app
        .project
        .document
        .table("TASKRSRC")
        .map_or(0, |t| t.rows.len())
}

#[test]
fn workflow_resource_assignments_delete_asks_first() {
    let p = views::resources::tests::with_resource();
    let mut d = Driver::new("wf-r3-assignment-delete", p);
    let before = assignment_rows(&d);
    assert!(before > 0, "the project has assignments");
    let steps = d.app.history.counts().0;
    d.home();
    d.open_menu("menu_bar_project");
    d.click("menu_resource_assignments");
    d.frames(3);
    assert_eq!(d.app.views.main, views::MainView::Assignments);
    // The first assignment row (under its resource band).
    let mut found = false;
    for i in 0..40 {
        if !d.has(&format!("assignments_row_{i}")) {
            break;
        }
        d.click(&format!("assignments_row_{i}"));
        if matches!(
            d.app.views.assignments.selected,
            Some(views::assignments::Key::Assignment(_))
        ) {
            found = true;
            break;
        }
    }
    assert!(found, "an assignment row is selected");
    d.key(Key::Delete);
    d.frames(2);
    assert!(
        d.has("delete_resource_assignment_yes"),
        "Delete asks first, as observed"
    );
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("No"),
        "No is the default, as observed"
    );
    d.key(Key::Enter);
    d.frames(2);
    assert_eq!(assignment_rows(&d), before, "Enter answers No");
    d.key(Key::Delete);
    d.frames(2);
    d.click("delete_resource_assignment_yes");
    d.frames(2);
    d.assert_clean("assignment delete");
    assert_eq!(assignment_rows(&d), before - 1, "Yes deletes it");
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(assignment_rows(&d), before, "undone");
}

/// The index of the first relationship drawn on the chart and a point on
/// its line away from every other line and from the bars.
fn drawn_link(d: &Driver) -> (usize, Pos2) {
    let bars: Vec<egui::Rect> = d
        .app
        .bar_geometry
        .iter()
        .map(|b| egui::Rect::from_two_pos(b.start, b.finish).expand(4.))
        .collect();
    let geometry = &d.app.link_geometry;
    let mut best: Option<(f32, usize, Pos2)> = None;
    for (index, points) in geometry {
        for pair in points.windows(2) {
            for t in [0.25, 0.5, 0.75] {
                let pos = pair[0].lerp(pair[1], t);
                if bars.iter().any(|r| r.contains(pos)) {
                    continue;
                }
                let clearance = geometry
                    .iter()
                    .filter(|(i, _)| i != index)
                    .flat_map(|(_, p)| p.windows(2))
                    .map(|p| crate::util::segment_distance(pos, p[0], p[1]))
                    .fold(f32::INFINITY, f32::min);
                if best.is_none_or(|(c, _, _)| clearance > c) {
                    best = Some((clearance, *index, pos));
                }
            }
        }
    }
    let (_, index, pos) = best.expect("a relationship line is drawn");
    (index, pos)
}

/// The center of activity `i`'s bar.
fn bar_center(d: &Driver, i: usize) -> Pos2 {
    let b = d
        .app
        .bar_geometry
        .iter()
        .find(|b| b.activity == i)
        .expect("the activity has a bar");
    Pos2::new((b.start.x + b.finish.x) / 2., b.row.center().y)
}

const CHART_MENU: [&str; 6] = [
    "Activity Details",
    "----",
    "Bars...",
    "Bar Chart Options...",
    "Timescale...",
    "Attachments",
];

fn chart_menu(d: &Driver) -> Vec<String> {
    crate::context_menu::drawn(&d.ctx)
        .into_iter()
        .filter(|e| e.prefix == "gantt_menu" && e.depth == 0)
        .map(|e| {
            if e.separator {
                "----".to_string()
            } else {
                e.label
            }
        })
        .collect()
}

#[test]
fn workflow_gantt_right_click_on_bars_milestones_and_lines() {
    let mut d = fresh("wf-r3-gantt-menus");
    d.home();
    d.app.gantt.options.show_relationships = true;
    d.frames(3);
    use openxer_engine::model::TaskType;
    let activities = &d.app.project.model.activities;
    let milestone = (0..activities.len())
        .find(|&i| matches!(activities[i].task_type, TaskType::Mile | TaskType::FinMile))
        .expect("the sample has a milestone");
    let task = d
        .app
        .bar_geometry
        .iter()
        .filter(|b| b.finish.x - b.start.x > 20.)
        .map(|b| b.activity)
        .find(|&i| i != milestone)
        .expect("a task bar");
    for (what, i) in [("a bar", task), ("a milestone", milestone)] {
        let at = bar_center(&d, i);
        d.press_at(at, egui::PointerButton::Secondary, Default::default());
        d.frames(2);
        assert_eq!(chart_menu(&d), CHART_MENU, "{what} opens the chart menu");
        assert!(d.app.link_editor.is_none(), "{what}: no Edit Relationship");
        d.key(Key::Escape);
        d.frames(2);
        assert!(d.app.link_editor.is_none(), "{what}: Escape only closes");
    }

    // A relationship line: selected, the same chart menu, then Edit
    // Relationship once the menu is closed with Escape.
    let (index, at) = drawn_link(&d);
    let links = d.app.project.model.links.clone();
    let steps = d.app.history.counts().0;
    d.press_at(at, egui::PointerButton::Secondary, Default::default());
    d.frames(2);
    assert_eq!(chart_menu(&d), CHART_MENU, "a line opens the chart menu");
    assert_eq!(
        d.app.gantt_ui.picked_link,
        Some(index),
        "the line is selected"
    );
    assert!(d.app.link_editor.is_none(), "not before the menu closes");
    d.key(Key::Escape);
    d.frames(2);
    assert!(chart_menu(&d).is_empty(), "Escape closes the menu");
    assert!(
        d.has("link_ok"),
        "then Edit Relationship opens, as observed"
    );
    // Delete there asks first; Yes (the default, Enter) deletes it.
    d.click("link_delete");
    assert!(d.has("delete_relationship_yes"), "Delete asks first");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Yes"),
        "Yes is the default, as for Remove on the Relationships tab"
    );
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("relationship delete");
    assert_eq!(d.app.project.model.links.len(), links.len() - 1);
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model.links, links, "undone");
}

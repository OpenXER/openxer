//! Workflow: the shared controls this look-and-feel pass moved onto the
//! kit. The Columns dialog's arrow buttons (`kit::move_button`) move a
//! column up and down and add and remove one, and OK puts the new order on
//! the activity table. The project's Data Date on Project Details, Dates
//! tab, is picked from the shared date field's calendar.
use super::*;

#[test]
fn workflow_data_date_is_picked_from_the_calendar() {
    let mut d = fresh("qa_data_date_picker");
    let id = new_project(&mut d, "DP-A", "Picker Unit", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Set pump skid", "5d");
    d.key_with(Key::S, ctrl());
    let minutes = |d: &Driver, text: &str| {
        openxer_xer::parse_relative(d.app.project.model.origin_serial, text)
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        d.app.project.model.project.data_date,
        Some(minutes(&d, "2027-01-04 08:00"))
    );

    // Projects, Dates tab: the Data Date has its `...` inside the edit.
    projects_select(&mut d, id);
    let tab = views::projects::TABS
        .iter()
        .position(|t| *t == "Dates")
        .unwrap();
    d.click(&format!("projects_tab_{tab}"));
    let field = d.rect("project_data_date");
    let button = d.rect("project_data_date_calendar");
    assert!(
        field.contains_rect(button),
        "the picker sits inside the edit frame: {field:?} {button:?}"
    );
    assert_eq!(button.right(), field.right() - 2.);

    // Pick the 20th from the calendar.
    let steps = d.app.history.counts().0;
    d.click("project_data_date_calendar");
    d.frames(2);
    assert!(d.click_text("20"), "the calendar shows January's days");
    d.frames(1);
    if d.app.project.model.project.data_date != Some(minutes(&d, "2027-01-20 08:00")) {
        // The calendar confirms a picked day with Save.
        assert!(d.click_text("Save"), "the calendar has Save");
    }
    d.frames(2);
    d.assert_clean("data date picker");
    assert_eq!(
        d.app.project.model.project.data_date,
        Some(minutes(&d, "2027-01-20 08:00")),
        "the picked day keeps the data date's time: {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
}

fn draft_ids(d: &Driver) -> Vec<String> {
    d.app
        .grid
        .dialogs
        .columns
        .as_ref()
        .expect("the Columns dialog is open")
        .columns
        .iter()
        .map(|c| c.id.clone())
        .collect()
}

fn table_ids(d: &Driver) -> Vec<String> {
    d.app
        .grid
        .layout
        .columns
        .iter()
        .map(|c| c.id.clone())
        .collect()
}

#[test]
fn workflow_columns_move_with_the_arrow_buttons() {
    let mut d = fresh("qa_columns_move_buttons");
    show(&mut d, "activities");
    let before = table_ids(&d);
    assert!(before.len() >= 3, "{before:?}");

    // View the Columns dialog from the toolbar.
    d.click("tool_columns");
    d.frames(2);
    assert!(d.app.grid.dialogs.columns.is_some(), "Columns opens");
    for key in [
        "columns_add",
        "columns_add_all",
        "columns_remove",
        "columns_remove_all",
        "columns_up",
        "columns_down",
    ] {
        let r = d.rect(key);
        assert_eq!((r.width(), r.height()), (33., 33.), "{key}");
    }

    // The second selected column moves up, then back down.
    d.click("columns_selected_1");
    d.click("columns_up");
    let mut swapped = before.clone();
    swapped.swap(0, 1);
    assert_eq!(draft_ids(&d), swapped, "up moves it above the first");
    d.click("columns_down");
    assert_eq!(draft_ids(&d), before, "down moves it back");
    // Down once more on the second column, then OK: the table follows.
    d.click("columns_selected_1");
    d.click("columns_down");
    let mut moved = before.clone();
    moved.swap(1, 2);
    assert_eq!(draft_ids(&d), moved);

    // Add a column with the right arrow and take it off with the left.
    d.click("columns_remove");
    let removed = draft_ids(&d);
    assert_eq!(removed.len(), moved.len() - 1, "left removes the column");
    let gone = moved
        .iter()
        .find(|id| !removed.contains(id))
        .unwrap()
        .clone();
    // Picked in Available Options (its group may be folded), then the
    // right arrow.
    d.app.grid.dialogs.columns.as_mut().unwrap().available = Some(gone.clone());
    d.frames(1);
    d.click("columns_add");
    let back = draft_ids(&d);
    assert!(back.contains(&gone), "right adds it back: {back:?}");
    assert_eq!(back.len(), moved.len());

    d.click("columns_ok");
    d.frames(2);
    d.assert_clean("columns");
    assert!(d.app.grid.dialogs.columns.is_none(), "OK closes");
    assert_eq!(table_ids(&d), back, "the table takes the new order");
}

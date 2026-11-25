use super::*;
use crate::qa_sweep::Driver;

fn titles(cols: &[(&str, f32, usize)]) -> Vec<String> {
    cols.iter().map(|c| c.0.to_string()).collect()
}

#[test]
fn defaults_are_the_grids_own_column_sets() {
    let d = Driver::new("grid-columns-defaults", openxer_xer::starter());
    for grid in ALL {
        let shown = d.app.detail_columns(grid, false);
        assert_eq!(titles(&shown), grid.default, "{}", grid.key);
        assert_eq!(d.app.detail_columns(grid, true).len(), grid.available.len());
        // Every default column is available and every title is unique.
        let mut seen = BTreeSet::new();
        assert!(
            grid.available.iter().all(|c| seen.insert(c.0)),
            "{}",
            grid.key
        );
    }
    assert_eq!(
        titles(&d.app.detail_columns(&STEPS, true)),
        [
            "Step Name",
            "% Complete",
            "Completed",
            "Step Weight",
            "Step Weight Percent",
        ]
    );
}

#[test]
fn customize_from_the_grid_menu_and_keep_the_choice() {
    let mut d = Driver::new(
        "grid-columns",
        crate::reports::test_support::scheduled_starter(),
    );
    d.app.selected = Some(0);
    d.app.show_detail_tab(DetailTab::Steps);
    d.frames(2);
    let model = d.app.project.model.clone();
    let undo = d.app.history.counts().0;
    // Right-click the grid above its Add button: the menu offers Customize.
    let add = d.rect("detail_steps_add");
    d.press_at(
        Pos2::new(add.left() + 60., add.top() - 30.),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    assert!(
        d.click_text("Customize Steps Columns..."),
        "menu item drawn"
    );
    d.frames(2);
    assert!(d.app.detail.column_dialog.is_some());
    // Cancel and Escape change nothing.
    d.click("detail_columns_cancel");
    assert!(d.app.detail.column_dialog.is_none());
    d.app.open_column_customize(&STEPS);
    d.key(Key::Escape);
    assert!(d.app.detail.column_dialog.is_none());
    assert!(d.app.detail.grid_columns.is_empty());
    // Add Step Weight after Step Name, drop % Complete, OK.
    d.app.open_column_customize(&STEPS);
    d.frames(2);
    d.click("detail_columns_selected_0");
    d.click("detail_columns_available_1");
    d.click("detail_columns_add");
    d.click("detail_columns_selected_2");
    d.click("detail_columns_remove");
    d.click("detail_columns_ok");
    assert_eq!(
        titles(&d.app.detail_columns(&STEPS, false)),
        ["Step Name", "Step Weight"]
    );
    assert!(d.app.project.model == model, "the project is untouched");
    assert_eq!(d.app.history.counts().0, undo);
    // The choice is kept in the workspace and read back.
    d.app.detail.grid_columns.clear();
    d.app.restore_grid_columns();
    assert_eq!(
        titles(&d.app.detail_columns(&STEPS, false)),
        ["Step Name", "Step Weight"]
    );
    // Default puts the grid's own set back.
    d.app.open_column_customize(&STEPS);
    d.frames(2);
    d.click("detail_columns_default");
    d.click("detail_columns_ok");
    assert!(d.app.detail.grid_columns.is_empty());
    assert_eq!(titles(&d.app.detail_columns(&STEPS, false)), STEPS.default);
    // The relationship grids draw the chosen columns too.
    d.app.detail.grid_columns.insert(
        "predecessors".into(),
        vec!["Activity ID".into(), "Critical".into()],
    );
    d.app.show_detail_tab(DetailTab::Predecessors);
    d.frames(2);
    assert!(d.texts().iter().any(|(t, _)| t == "Critical"));
    assert!(d.app.error.is_none());
}

#[test]
fn relationship_comments_expense_account_and_risk_columns_show_the_xer_values() {
    let mut d = Driver::new("grid-columns-xer-values", crate::l2_renders::project());
    d.app.selected = Some(1);
    let shown = |d: &mut Driver, text: &str| d.texts().iter().any(|(t, _)| t == text);
    // The relationship comment is on the row of RB-1000 > RB-1010.
    d.app.show_detail_tab(DetailTab::Predecessors);
    d.frames(2);
    assert!(
        shown(&mut d, "Crane booked for the lift"),
        "relationship Comments"
    );
    // Expense: Cost Account (short name) and Auto Compute Actuals (checked).
    d.app.show_detail_tab(DetailTab::Expenses);
    d.frames(2);
    assert!(shown(&mut d, "Crane rental"));
    assert!(shown(&mut d, "2100"), "expense Cost Account");
    // Risk: category, type, status and owner.
    d.app.show_detail_tab(DetailTab::Risks);
    d.frames(2);
    for text in [
        "R-1",
        "Crane wind limit",
        "Safety",
        "Threat",
        "Open",
        "Pipefitter",
    ] {
        assert!(shown(&mut d, text), "{text}");
    }
    // The values come from the file, not from the grid: undo does not matter
    // and a relationship without a row reads blank.
    let links = [0usize];
    let comments = d.app.project.relationship_comments(&links);
    assert_eq!(comments, ["Crane booked for the lift"]);
    assert_eq!(
        d.app.project.relationship_comments(&[]),
        Vec::<String>::new()
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

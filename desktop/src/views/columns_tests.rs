//! Customize Columns on the project record views and Customize Resource
//! Rate Columns on the Resources view's rate grid, through the production
//! UI: the shared column customizer opens from the right-click menu, OK
//! applies, Cancel and Escape do not, Default puts the list's own columns
//! back, and the choice is kept with the view layout.
use super::*;
use crate::qa_sweep::Driver;
use egui::{Key, Pos2};
use openxer_xer::starter;

fn right_click(d: &mut Driver, pos: Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

/// A point inside the view table `id`, below its header.
fn table_point(d: &Driver, id: &str) -> Pos2 {
    match d.app.action_rects.get(&format!("{id}_row_0")) {
        Some(r) => Pos2::new(r[0] + 90., (r[1] + r[3]) / 2.),
        None => {
            let r = d.rect(&format!("{id}_layout"));
            Pos2::new(r.center().x, r.bottom() + 60.)
        }
    }
}

fn open_customize(d: &mut Driver, id: &str) {
    let at = table_point(d, id);
    right_click(d, at);
    d.click(&format!("{id}_menu_customize"));
    d.frames(2);
    assert!(
        d.app.detail.column_dialog.is_some(),
        "{id}: the dialog opens"
    );
}

fn titles_shown(d: &mut Driver) -> Vec<String> {
    d.app
        .detail
        .column_dialog
        .as_ref()
        .unwrap()
        .selected
        .clone()
}

#[test]
fn every_record_view_customizes_its_columns_and_keeps_them_with_the_layout() {
    for (view, id, titles) in [
        (MainView::Expenses, "expenses", &expenses::TITLES[..]),
        (MainView::Documents, "documents", &documents::TITLES[..]),
        (MainView::Risks, "risks", &risks::TITLES[..]),
        (MainView::Issues, "issues", &issues::TITLES[..]),
        (MainView::Thresholds, "thresholds", &thresholds::TITLES[..]),
    ] {
        let mut d = Driver::new(&format!("cols-{id}"), starter());
        d.app.show_view(view);
        d.frames(3);
        let order = |d: &mut Driver| {
            record_view::list_state(&mut d.app.views, id)
                .unwrap()
                .columns
                .order
                .clone()
        };
        let all: Vec<usize> = (0..titles.len()).collect();
        assert_eq!(order(&mut d), all, "{id}: every column at first");
        open_customize(&mut d, id);
        assert_eq!(titles_shown(&mut d), titles, "{id}: Selected Options");
        // Remove the second column, then move the new second one up.
        d.click("detail_columns_selected_1");
        d.click("detail_columns_remove");
        d.click("detail_columns_selected_1");
        d.click("detail_columns_up");
        // Cancel keeps the view as it was.
        d.click("detail_columns_cancel");
        d.frames(2);
        assert!(d.app.detail.column_dialog.is_none());
        assert_eq!(order(&mut d), all, "{id}: Cancel changes nothing");
        // Again, with OK.
        open_customize(&mut d, id);
        d.click("detail_columns_selected_1");
        d.click("detail_columns_remove");
        d.click("detail_columns_selected_1");
        d.click("detail_columns_up");
        d.click("detail_columns_ok");
        d.frames(2);
        let mut want: Vec<usize> = vec![2, 0];
        want.extend(3..titles.len());
        assert_eq!(order(&mut d), want, "{id}: OK applies");
        assert!(
            !d.texts().iter().any(|(t, _)| t == titles[1]),
            "{id}: the removed column {:?} is not drawn",
            titles[1]
        );
        // Kept with the view layout: the stored views restore it.
        let json = d.app.views.json();
        let mut back = Views::default();
        back.restore(&json);
        assert_eq!(
            record_view::list_state(&mut back, id)
                .unwrap()
                .columns
                .order,
            want,
            "{id}: saved with the layout"
        );
        // Escape closes; Default puts every column back.
        open_customize(&mut d, id);
        d.key(Key::Escape);
        d.frames(2);
        assert!(d.app.detail.column_dialog.is_none(), "{id}: Escape closes");
        assert_eq!(order(&mut d), want);
        open_customize(&mut d, id);
        d.click("detail_columns_default");
        d.click("detail_columns_ok");
        d.frames(2);
        assert_eq!(order(&mut d), all, "{id}: Default");
        assert!(d.app.error.is_none(), "{id}: {:?}", d.app.error);
    }
}

#[test]
fn resource_rate_columns_add_the_other_prices_and_edit_them() {
    let mut d = Driver::new("cols-rates", resources::tests::with_resource());
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    let id = 7;
    d.app.views.resources.selected = Some(id);
    d.app.views.resources.show_bottom = true;
    d.app.views.resources.tab = 3;
    d.frames(3);
    let at = d.rect("resource_rates_row_0").center();
    right_click(&mut d, at);
    d.click("resource_rates_menu_customize");
    d.frames(2);
    assert_eq!(
        titles_shown(&mut d),
        resources::RATE_DEFAULT,
        "the rate grid's own columns"
    );
    // Add Price / Unit 2 (the first available option) after Price / Unit.
    d.click("detail_columns_selected_2");
    d.click("detail_columns_available_0");
    d.click("detail_columns_add");
    d.click("detail_columns_ok");
    d.frames(3);
    assert_eq!(
        d.app.views.resources.rate_columns,
        [
            "Effective Date",
            "Max Units / Time",
            "Price / Unit",
            "Price / Unit 2"
        ]
    );
    assert!(d.texts().iter().any(|(t, _)| t == "Price / Unit 2"));
    // The new column edits RSRCRATE.cost_per_qty2.
    d.click("resource_rate_cell_3");
    d.key_with(
        Key::A,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    d.type_text("85");
    d.key(Key::Enter);
    d.frames(2);
    let t = d.app.project.document.table("RSRCRATE").unwrap();
    let row = crate::views::resource_edit::rate_rows(&d.app.project.document, id)[0];
    assert_eq!(
        crate::records::float(t.value(&t.rows[row], "cost_per_qty2")),
        85.
    );
    // Kept with the view layout; an unknown stored title is ignored.
    let json = d.app.views.json();
    let mut back = Views::default();
    back.restore(&json);
    assert_eq!(
        back.resources.rate_columns,
        d.app.views.resources.rate_columns
    );
    let mut bad = serde_json::from_str::<serde_json::Value>(&json).unwrap();
    bad["resources"]["rate_columns"] = serde_json::json!(["Effective Date", "Bogus"]);
    let mut kept = Views::default();
    kept.restore(&bad.to_string());
    assert!(kept.resources.rate_columns.is_empty());
    // Default goes back to the three columns.
    right_click(&mut d, at);
    d.click("resource_rates_menu_customize");
    d.frames(2);
    d.click("detail_columns_default");
    d.click("detail_columns_ok");
    d.frames(2);
    assert!(d.app.views.resources.rate_columns.is_empty());
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

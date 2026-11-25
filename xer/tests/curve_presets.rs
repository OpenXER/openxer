//! Preset resource curves: listed once whatever the file holds, copied into a
//! file only when an assignment uses one, and stable over save and reload.
use openxer_xer::edit::curves::{self, Kind};
use openxer_xer::{starter, Document};

fn with_assignments(n: usize) -> Document {
    let mut doc = starter().document;
    for k in 0..n {
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", (500 + k).to_string()),
            ("task_id", "1040".into()),
            ("proj_id", "1".into()),
            ("rsrc_id", "10".into()),
        ]);
    }
    doc
}

fn reload(doc: &Document) -> Document {
    Document::from_bytes(&doc.to_bytes().unwrap()).unwrap()
}

fn rows(doc: &Document) -> usize {
    doc.table(curves::TABLE).map_or(0, |t| t.rows.len())
}

fn count(doc: &Document, name: &str) -> usize {
    curves::list(doc)
        .iter()
        .filter(|c| c.name.eq_ignore_ascii_case(name))
        .count()
}

#[test]
fn every_document_lists_each_preset_once_without_storing_them() {
    let doc = with_assignments(0);
    let rows_before = rows(&doc);
    let list = curves::list(&doc);
    for p in curves::presets() {
        assert_eq!(count(&doc, p.name), 1, "{}", p.name);
    }
    assert!(list
        .iter()
        .filter(|c| c.id.is_none())
        .all(|c| c.kind == Kind::Preset && c.locked()));
    // Listing writes nothing, so an untouched file exports as it was.
    assert_eq!(rows(&doc), rows_before);
    assert_eq!(doc.to_bytes().unwrap(), reload(&doc).to_bytes().unwrap());
}

#[test]
fn a_preset_is_copied_once_and_survives_repeated_save_and_reload() {
    let mut doc = with_assignments(3);
    let row = doc.table("TASKRSRC").unwrap().rows.len() - 3;
    assert!(curves::set_assignment_curve(&mut doc, row, Some("bell, centered")).unwrap());
    let id = curves::of_assignment(&doc, row).unwrap();
    assert!(curves::set_assignment_curve(&mut doc, row + 1, Some("Bell, Centered")).unwrap());
    assert_eq!(curves::of_assignment(&doc, row + 1), Some(id));
    assert_eq!(rows(&doc), 1, "the preset is stored once");
    // Same value again is no change.
    assert!(!curves::set_assignment_curve(&mut doc, row, Some("Bell, Centered")).unwrap());
    for _ in 0..3 {
        doc = reload(&doc);
        assert_eq!(rows(&doc), 1);
        assert_eq!(count(&doc, "Bell, Centered"), 1);
        let c = curves::find(&doc, "Bell, Centered").unwrap();
        assert_eq!(c.id, Some(id));
        assert_eq!(c.kind, Kind::Preset, "still recognised as the preset");
        assert_eq!(
            c.points,
            curves::preset("Bell, Centered").unwrap().points,
            "points round trip exactly"
        );
        assert_eq!(curves::of_assignment(&doc, row), Some(id));
        // Using it again after a reload does not add a second row.
        curves::set_assignment_curve(&mut doc, row + 2, Some("Bell, Centered")).unwrap();
        assert_eq!(rows(&doc), 1);
    }
    // The exported row says it is not a default curve.
    assert_eq!(
        openxer_xer::edit::records::get(&doc, curves::TABLE, 0, "default_flag"),
        "N"
    );
    // Clearing leaves the assignment with no curve.
    assert!(curves::set_assignment_curve(&mut doc, row, None).unwrap());
    assert_eq!(curves::of_assignment(&doc, row), None);
}

#[test]
fn a_files_own_curve_with_a_preset_name_wins_and_ids_do_not_move() {
    let mut doc = with_assignments(2);
    let mut values = vec![
        ("curv_id", "7".to_string()),
        ("curv_name", "Even".to_string()),
        ("default_flag", "N".to_string()),
    ];
    let names: Vec<String> = (0..curves::POINTS)
        .map(|k| format!("pct_usage_{k}"))
        .collect();
    for (k, n) in names.iter().enumerate() {
        values.push((n.as_str(), if k == 20 { "100" } else { "0" }.to_string()));
    }
    doc.table_mut(curves::TABLE).add(&values);
    let row = doc.table("TASKRSRC").unwrap().rows.len() - 2;
    openxer_xer::edit::records::set(&mut doc, "TASKRSRC", row, "curv_id", "7".into());
    let mut doc = reload(&doc);
    assert_eq!(count(&doc, "Even"), 1);
    let even = curves::find(&doc, "even").unwrap();
    assert_eq!(even.id, Some(7));
    assert_eq!(even.kind, Kind::File, "the file's curve, editable");
    assert_eq!(even.points[20], 100.);
    // Naming it picks the file's curve; nothing is added.
    curves::set_assignment_curve(&mut doc, row + 1, Some("Even")).unwrap();
    assert_eq!(curves::of_assignment(&doc, row + 1), Some(7));
    assert_eq!(rows(&doc), 1);
    // A preset copied in later takes a new ID; the used curve keeps 7.
    curves::set_assignment_curve(&mut doc, row + 1, Some("Ramp Up")).unwrap();
    let ramp = curves::of_assignment(&doc, row + 1).unwrap();
    assert_eq!(ramp, 8);
    assert_eq!(curves::of_assignment(&doc, row), Some(7));
    assert_eq!(curves::by_id(&doc, 7).unwrap().name, "Even");
}

#[test]
fn unknown_curve_names_are_refused_and_ids_still_accepted() {
    let mut doc = with_assignments(1);
    let row = doc.table("TASKRSRC").unwrap().rows.len() - 1;
    let err = curves::set_assignment_curve(&mut doc, row, Some("Wobbly")).unwrap_err();
    assert!(err.contains("Wobbly") && err.contains("Even"), "{err}");
    assert_eq!(rows(&doc), 0);
    let id = curves::ensure(&mut doc, "Two Peaks").unwrap();
    curves::set_assignment_curve(&mut doc, row, Some(&id.to_string())).unwrap();
    assert_eq!(curves::of_assignment(&doc, row), Some(id));
    assert!(curves::set_assignment_curve(&mut doc, row + 5, Some("Even")).is_err());
}

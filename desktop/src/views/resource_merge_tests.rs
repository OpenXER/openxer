//! Resources view > Del. / Merge through the production UI: the dialog
//! opens from the right-click menu on the selected resource, previews the
//! merge, merges as one undo step, deletes an unused resource, closes on
//! Escape without changes, and the merged file survives an XER round trip.
use super::*;
use crate::qa_sweep::Driver;
use egui::{Key, Pos2};
use openxer_xer::edit::resources as merge;
use openxer_xer::{Document, ProjectData};

/// The resource fixture plus PIPE-2, a duplicate Pipefitter assigned to
/// 1010 (with PIPE) and to 1020.
pub(crate) fn duplicates() -> ProjectData {
    let mut p = resources::tests::with_resource();
    let d = &mut p.document;
    d.table_mut("RSRC").add(&[
        ("rsrc_id", "11".into()),
        ("clndr_id", "1".into()),
        ("rsrc_short_name", "PIPE-2".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("rsrc_seq_num", "4".into()),
    ]);
    for (id, task, qty) in [("21", "1010", "16"), ("22", "1020", "8")] {
        d.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", id.into()),
            ("task_id", task.into()),
            ("proj_id", "1".into()),
            ("rsrc_id", "11".into()),
            ("remain_qty", qty.into()),
            ("target_qty", qty.into()),
            ("remain_qty_per_hr", "1".into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    ProjectData::from_document(p.document, 1, "merge-ui".into()).unwrap()
}

fn open(d: &mut Driver, rsrc: i64) {
    d.click("directory_resources");
    d.app.views.resources.assigned_only = false;
    d.app.views.resources.selected = Some(rsrc);
    d.frames(3);
    let r = d.rect("resources_row_0");
    d.press_at(
        Pos2::new(r.left() + 90., r.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    // The right-click selected row 0; put the wanted resource back.
    d.app.views.resources.selected = Some(rsrc);
    d.click("resources_menu_del_merge");
    d.frames(2);
}

fn assignments_of(p: &ProjectData, rsrc: i64) -> usize {
    merge::assignment_count(&p.document, rsrc)
}

#[test]
fn del_merge_previews_merges_in_one_undo_step_and_round_trips() {
    let mut d = Driver::new("resource-merge", duplicates());
    open(&mut d, 11);
    let dialog = d.app.views.resources.merge.clone().expect("dialog opens");
    assert_eq!(dialog.resource, 11);
    assert!(dialog.merging, "an assigned resource opens on Merge");
    assert_eq!(dialog.target, Some(7), "the same-named resource is picked");
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(
        texts.iter().any(|t| t.contains("Merge PIPE-2 into PIPE")),
        "preview: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("RB-1010")),
        "the combined activity is named: {texts:?}"
    );
    // Nothing changed yet.
    assert_eq!(assignments_of(&d.app.project, 11), 2);
    let steps = d.app.history.steps();
    d.click("resource_merge_apply");
    d.frames(2);
    assert!(d.app.views.resources.merge.is_none(), "applying closes it");
    assert!(merge::row(&d.app.project.document, 11).is_none());
    assert_eq!(
        assignments_of(&d.app.project, 7),
        2,
        "1010 combined, 1020 moved"
    );
    assert_eq!(d.app.history.steps(), steps + 1, "one undo step");
    assert_eq!(d.app.views.resources.selected, Some(7));
    assert!(
        d.app.status.contains("Merged PIPE-2 into PIPE"),
        "{}",
        d.app.status
    );
    // The merged project exports and reads back the same.
    let text = d.app.project.updated_document().text();
    let back = ProjectData::from_document(Document::parse(&text).unwrap(), 1, "rt".into()).unwrap();
    assert_eq!(assignments_of(&back, 7), 2);
    assert!(merge::row(&back.document, 11).is_none());
    // Undo brings the duplicate and its assignments back.
    d.app.history_move(&d.ctx.clone(), false);
    d.frames(2);
    assert!(merge::row(&d.app.project.document, 11).is_some());
    assert_eq!(assignments_of(&d.app.project, 11), 2);
    assert_eq!(assignments_of(&d.app.project, 7), 1);
}

#[test]
fn del_merge_ticks_more_duplicates_and_escape_changes_nothing() {
    let mut d = Driver::new("resource-merge-esc", duplicates());
    open(&mut d, 11);
    let before = d.app.project.document.text();
    // Pick IDLE? It is material: only labor resources are listed.
    let listed: Vec<i64> = merge_candidates(&d, 11);
    assert_eq!(listed, vec![7], "only same-type resources: {listed:?}");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.views.resources.merge.is_none(), "Escape closes it");
    assert_eq!(d.app.project.document.text(), before);
    // Cancel too.
    open(&mut d, 11);
    d.click("resource_merge_cancel");
    d.frames(2);
    assert!(d.app.views.resources.merge.is_none());
    assert_eq!(d.app.project.document.text(), before);
}

fn merge_candidates(d: &Driver, rsrc: i64) -> Vec<i64> {
    resource_merge::candidates(&d.app.project, rsrc)
        .into_iter()
        .map(|r| r.id)
        .collect()
}

#[test]
fn del_merge_deletes_an_unused_resource_in_one_step() {
    let mut d = Driver::new("resource-delete", duplicates());
    open(&mut d, 9);
    let dialog = d.app.views.resources.merge.clone().expect("dialog opens");
    assert!(!dialog.merging, "an unused resource opens on Delete");
    let steps = d.app.history.steps();
    d.click("resource_merge_apply");
    d.frames(2);
    assert!(merge::row(&d.app.project.document, 9).is_none());
    assert_eq!(d.app.history.steps(), steps + 1);
    d.app.history_move(&d.ctx.clone(), false);
    d.frames(2);
    assert!(merge::row(&d.app.project.document, 9).is_some());
}

#[test]
fn del_merge_merges_ticked_duplicates_too() {
    let mut p = duplicates();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "12".into()),
        ("rsrc_short_name", "PIPE-3".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_type", "RT_Labor".into()),
    ]);
    let mut d = Driver::new("resource-merge-more", p);
    open(&mut d, 11);
    // Rows: PIPE (7), PIPE-3 (12) by Resource ID.
    assert_eq!(merge_candidates(&d, 11), vec![7, 12]);
    d.click("resource_merge_also_1");
    d.frames(2);
    let dialog = d.app.views.resources.merge.clone().unwrap();
    assert_eq!(dialog.sources(), vec![11, 12]);
    d.click("resource_merge_apply");
    d.frames(2);
    let doc = &d.app.project.document;
    assert!(merge::row(doc, 11).is_none() && merge::row(doc, 12).is_none());
    assert!(merge::row(doc, 7).is_some());
}

#[test]
fn del_merge_keeps_its_preview_and_says_how_much_more_there_is() {
    // Eight more labor duplicates, each assigned to 1020.
    let mut p = duplicates();
    for n in 0..8_i64 {
        let id = 40 + n;
        p.document.table_mut("RSRC").add(&[
            ("rsrc_id", id.to_string()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", format!("PIPE-X{n}")),
            ("rsrc_name", "Pipefitter".into()),
            ("rsrc_type", "RT_Labor".into()),
            ("rsrc_seq_num", (10 + n).to_string()),
        ]);
        p.document.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", (60 + n).to_string()),
            ("task_id", "1020".into()),
            ("proj_id", "1".into()),
            ("rsrc_id", id.to_string()),
            ("remain_qty", "8".into()),
            ("target_qty", "8".into()),
            ("remain_qty_per_hr", "1".into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    let p = ProjectData::from_document(p.document, 1, "merge-many".into()).unwrap();
    let mut d = Driver::new("resource-merge-many", p);
    open(&mut d, 11);
    let mut dialog = d.app.views.resources.merge.clone().unwrap();
    dialog.also = (40..48).collect();
    d.app.views.resources.merge = Some(dialog);
    d.frames(2);
    // The preview is computed once, not every frame.
    let hits = d.app.views.memo.borrow().hits;
    d.frames(3);
    assert!(
        d.app.views.memo.borrow().hits >= hits + 3,
        "the preview is kept between frames"
    );
    let texts: Vec<String> = d.texts().into_iter().map(|(t, _)| t).collect();
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("and ") && t.contains(" more line")),
        "a cut preview says so: {texts:?}"
    );
    d.key(Key::Escape);
}

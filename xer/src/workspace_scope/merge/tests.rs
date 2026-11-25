use super::*;
use crate::workspace_scope::tests::{synthetic, xer};

fn migration(label: &str) -> Policy {
    Policy::Migration {
        label: label.into(),
    }
}

fn none() -> BTreeSet<(String, i64)> {
    BTreeSet::new()
}

fn value(doc: &Document, table: &str, key: &str, id: i64, column: &str) -> String {
    let t = doc.table(table).unwrap();
    let r = t
        .rows
        .iter()
        .find(|r| num(t.value(r, key)) == id)
        .unwrap_or_else(|| panic!("{table} {id}"));
    t.value(r, column).to_string()
}

fn ids(doc: &Document, table: &str) -> Vec<i64> {
    let t = doc.table(table).unwrap();
    let k = key_index(t).unwrap();
    t.rows.iter().map(|r| num(&r[k])).collect()
}

fn set(doc: &mut Document, table: &str, key: &str, id: i64, column: &str, text: &str) {
    let t = doc.table_mut(table);
    let i = t
        .rows
        .iter()
        .position(|r| num(t.value(r, key)) == id)
        .unwrap();
    t.set(i, column, text.into());
}

/// Ids project rows of `doc` keep in the tables of both kinds.
fn reserve_projects(m: &mut Merger, docs: &[&Document]) {
    for doc in docs {
        let s = Scopes::of(doc);
        for table in MIXED_TABLES {
            if let Some(t) = doc.table(table) {
                let k = key_index(t).unwrap();
                for r in &t.rows {
                    if s.scope(t, r) != Scope::Workspace {
                        m.reserve(table, num(&r[k]));
                    }
                }
            }
        }
    }
}

#[test]
fn every_workspace_table_is_merged_by_some_rule() {
    for (table, _) in WORKSPACE_TABLES {
        let covered =
            ORDER.contains(table) || value_group(table).is_some() || owner_of(table).is_some();
        assert!(covered, "{table}");
    }
    for t in ORDER {
        assert!(is_workspace_table(t), "{t}");
    }
}

#[test]
fn the_same_dictionary_in_two_projects_is_stored_once() {
    let a = synthetic(1, "ALPHA");
    let b = synthetic(2, "BETA");
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    reserve_projects(&mut m, &[&a, &b]);
    let first = m.merge(&a, &migration("ALPHA"), &none());
    assert!(first.remap.is_empty(), "{:?}", first.remap);
    assert_eq!(split(&a).1.text(), {
        let mut d = m.dict.clone();
        d.header = a.header.clone();
        d.text()
    });
    let second = m.merge(&b, &migration("BETA"), &none());
    assert!(second.remap.is_empty(), "{:?}", second.remap);
    assert!(second
        .events
        .iter()
        .all(|e| e.kind == EventKind::Merged || e.table == "ACTVCODE"));
    assert_eq!(ids(&m.dict, "CALENDAR"), [1, 2, 4]);
    assert_eq!(ids(&m.dict, "RSRC"), [7, 8]);
    assert_eq!(ids(&m.dict, "ACTVCODE"), [41, 42]);
    assert_eq!(second.document.text(), b.text());
}

#[test]
fn a_calendar_named_like_a_different_one_becomes_a_renamed_copy() {
    let a = synthetic(1, "ALPHA");
    let mut b = synthetic(2, "BETA");
    set(
        &mut b,
        "CALENDAR",
        "clndr_id",
        2,
        "clndr_data",
        "(0||CalendarData()())",
    );
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    reserve_projects(&mut m, &[&a, &b]);
    m.merge(&a, &migration("ALPHA"), &none());
    let out = m.merge(&b, &migration("BETA"), &none());
    let copy = out
        .events
        .iter()
        .find(|e| e.kind == EventKind::Copied && e.table == "CALENDAR")
        .expect("a copy");
    assert_eq!(copy.renamed.as_deref(), Some("Standard (BETA)"));
    assert_eq!(copy.differences, ["clndr_data"]);
    // Ids 1 to 4 are taken (3 by each project's own calendar), so the copy is 5.
    assert_eq!(copy.to, 5);
    assert_eq!(
        value(&out.document, "PROJECT", "proj_id", 2, "clndr_id"),
        "5"
    );
    assert_eq!(
        value(&out.document, "TASK", "task_id", 102, "clndr_id"),
        "5"
    );
    assert_eq!(
        value(&out.document, "CALENDAR", "clndr_id", 3, "base_clndr_id"),
        "5"
    );
    assert_eq!(
        value(&m.dict, "CALENDAR", "clndr_id", 5, "clndr_name"),
        "Standard (BETA)"
    );
    assert_eq!(
        value(&m.dict, "CALENDAR", "clndr_id", 2, "clndr_name"),
        "Standard"
    );
    // The helper works on that calendar, so it is a different resource too.
    let helper = out
        .events
        .iter()
        .find(|e| e.table == "RSRC" && e.from == 8)
        .unwrap();
    assert_eq!(helper.kind, EventKind::Copied);
    assert_eq!(helper.renamed.as_deref(), Some("HLP (BETA)"));
    assert_eq!(
        out.events
            .iter()
            .find(|e| e.table == "RSRC" && e.from == 7)
            .unwrap()
            .kind,
        EventKind::Merged
    );
}

#[test]
fn resources_with_different_limits_are_never_merged() {
    let a = synthetic(1, "ALPHA");
    let mut b = synthetic(2, "BETA");
    set(
        &mut b,
        "RSRCRATE",
        "rsrc_rate_id",
        31,
        "max_qty_per_hr",
        "3",
    );
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    reserve_projects(&mut m, &[&a, &b]);
    m.merge(&a, &migration("ALPHA"), &none());
    let out = m.merge(&b, &migration("BETA"), &none());
    let op = out
        .events
        .iter()
        .find(|e| e.table == "RSRC" && e.from == 7)
        .unwrap();
    assert_eq!(op.kind, EventKind::Copied);
    let new = op.to;
    let rates = m.dict.table("RSRCRATE").unwrap();
    let limits: Vec<(i64, String)> = rates
        .rows
        .iter()
        .map(|r| {
            (
                num(rates.value(r, "rsrc_id")),
                rates.value(r, "max_qty_per_hr").to_string(),
            )
        })
        .collect();
    assert!(limits.contains(&(7, "1".into())));
    assert!(limits.contains(&(new, "3".into())));
    assert_eq!(limits.iter().filter(|(r, _)| *r == 7).count(), 1);
    assert_eq!(
        value(&out.document, "TASKRSRC", "taskrsrc_id", 201, "rsrc_id"),
        new.to_string()
    );
    assert_eq!(
        value(&out.document, "UDFVALUE", "udf_type_id", 7, "fk_id"),
        new.to_string()
    );
}

#[test]
fn code_types_share_values_and_a_conflicting_value_copies_the_type() {
    let a = synthetic(1, "ALPHA");
    let mut b = synthetic(2, "BETA");
    b.table_mut("ACTVCODE").rows.push(
        ["43", "", "40", "Steel", "STL", "3", "0"]
            .map(String::from)
            .to_vec(),
    );
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    reserve_projects(&mut m, &[&a, &b]);
    m.merge(&a, &migration("ALPHA"), &none());
    let out = m.merge(&b, &migration("BETA"), &none());
    assert!(out.remap.is_empty());
    assert_eq!(ids(&m.dict, "ACTVCODE"), [41, 42, 43]);
    assert_eq!(ids(&m.dict, "ACTVTYPE"), [40]);

    let mut c = synthetic(3, "GAMMA");
    set(
        &mut c,
        "ACTVCODE",
        "actv_code_id",
        42,
        "actv_code_name",
        "Cast in place",
    );
    let schedule = xer(&[(
        "SCHEDOPTIONS",
        "schedoptions_id proj_id LevelPriorityList",
        &["1|3|ACTVCODE_PHASE,ASC\u{7f}priority_type,DESC"],
    )]);
    c.tables
        .push(schedule.table("SCHEDOPTIONS").unwrap().clone());
    reserve_projects(&mut m, &[&c]);
    let out = m.merge(&c, &migration("GAMMA"), &none());
    let ty = out.events.iter().find(|e| e.table == "ACTVTYPE").unwrap();
    assert_eq!(ty.kind, EventKind::Copied);
    assert_eq!(ty.renamed.as_deref(), Some("Phase (GAMMA)"));
    assert_eq!(
        value(
            &out.document,
            "SCHEDOPTIONS",
            "schedoptions_id",
            1,
            "LevelPriorityList"
        ),
        "ACTVCODE_PHASE_GAMMA,ASC\u{7f}priority_type,DESC",
        "leveling priorities follow the renamed type"
    );
    let codes = m.dict.table("ACTVCODE").unwrap();
    let under_copy = codes
        .rows
        .iter()
        .filter(|r| num(codes.value(r, "actv_code_type_id")) == ty.to)
        .count();
    assert_eq!(under_copy, 2, "the copy carries the project's values");
}

#[test]
fn a_project_row_never_shares_an_id_with_a_workspace_row() {
    // ALPHA's project calendar 3 is reserved; BETA brings a global calendar 3.
    let a = synthetic(1, "ALPHA");
    let b = xer(&[
        (
            "PROJECT",
            "proj_id proj_short_name last_recalc_date clndr_id",
            &["2|BETA|2026-11-02 08:00|3"],
        ),
        (
            "CALENDAR",
            "clndr_id default_flag clndr_name proj_id base_clndr_id clndr_type clndr_data",
            &["3|N|Weekend|||CA_Base|x"],
        ),
    ]);
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    reserve_projects(&mut m, &[&a, &b]);
    m.merge(&a, &migration("ALPHA"), &none());
    let out = m.merge(&b, &migration("BETA"), &none());
    let e = &out.events[0];
    assert_eq!((e.kind, e.from), (EventKind::Renumbered, 3));
    assert_eq!(e.to, 5);
    assert_eq!(
        value(&out.document, "PROJECT", "proj_id", 2, "clndr_id"),
        "5"
    );
}

#[test]
fn saving_updates_the_rows_a_project_read_and_adds_its_new_ones() {
    let a = synthetic(1, "ALPHA");
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    let first = m.merge(&a, &Policy::Save, &none());
    let mark = first.mark.clone();
    assert!(mark.contains(&("CALENDAR".into(), 2)));
    assert!(
        !mark.contains(&("CALENDAR".into(), 3)),
        "project rows are not marked"
    );
    let floor = table_highs(&m.dict);
    m.set_floor(&floor);
    // The project renames its calendar and adds a resource.
    let mut edited = a.clone();
    set(
        &mut edited,
        "CALENDAR",
        "clndr_id",
        2,
        "clndr_name",
        "Standard 5x8",
    );
    edited.table_mut("RSRC").rows.push(
        ["9", "", "2", "Welder", "WLD", "1", "g-w"]
            .map(String::from)
            .to_vec(),
    );
    m.changed = false;
    let out = m.merge(&edited, &Policy::Save, &mark);
    assert!(m.changed);
    assert!(out.remap.is_empty());
    assert_eq!(
        value(&m.dict, "CALENDAR", "clndr_id", 2, "clndr_name"),
        "Standard 5x8"
    );
    assert_eq!(
        value(&m.dict, "RSRC", "rsrc_id", 9, "rsrc_short_name"),
        "WLD"
    );
    // Another project, never hydrated, brings a different "WLD" under the
    // same id and an identical "OP" under another id.
    let other = xer(&[
        (
            "PROJECT",
            "proj_id proj_short_name last_recalc_date clndr_id",
            &["2|BETA|2026-11-02 08:00|2"],
        ),
        (
            "RSRC",
            "rsrc_id parent_rsrc_id clndr_id rsrc_name rsrc_short_name curr_id guid",
            &["9||2|Welder 2|WLD|1|g-w2"],
        ),
        (
            "TASKRSRC",
            "taskrsrc_id task_id proj_id rsrc_id",
            &["5|1|2|9"],
        ),
    ]);
    let out = m.merge(&other, &Policy::Save, &none());
    let e = out.events.iter().find(|e| e.table == "RSRC").unwrap();
    assert_eq!(e.kind, EventKind::Copied);
    assert_eq!(e.renamed.as_deref(), Some("WLD_1"));
    assert_eq!(e.to, 10);
    assert_eq!(
        value(&out.document, "TASKRSRC", "taskrsrc_id", 5, "rsrc_id"),
        "10"
    );
    assert_eq!(value(&m.dict, "RSRC", "rsrc_id", 9, "rsrc_name"), "Welder");
    assert!(out.mark.is_empty() || !out.mark.contains(&("RSRC".into(), 9)));
}

#[test]
fn saving_again_after_a_renumber_adds_nothing() {
    let a = synthetic(1, "ALPHA");
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    m.merge(&a, &Policy::Save, &none());
    let mut other = synthetic(2, "BETA");
    set(
        &mut other,
        "CALENDAR",
        "clndr_id",
        2,
        "clndr_data",
        "changed",
    );
    let once = m.merge(&other, &Policy::Save, &none());
    let text = m.dict.text();
    m.changed = false;
    let twice = m.merge(&other, &Policy::Save, &once.mark);
    assert_eq!(m.dict.text(), text);
    assert!(!m.changed);
    assert_eq!(once.document.text(), twice.document.text());
}

#[test]
fn removing_a_resource_removes_its_category_rows() {
    let doc = xer(&[
        ("RSRC", "rsrc_id rsrc_short_name", &["7|OP"]),
        (
            "RSRCRCAT",
            "rsrc_id rsrc_catg_type_id rsrc_catg_id",
            &["7|1|2"],
        ),
    ]);
    let mut m = Merger::new(Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    m.merge(&doc, &Policy::Save, &none());
    assert_eq!(m.dict.table("RSRCRCAT").unwrap().rows.len(), 1);
    m.remove("RSRC", 7);
    assert!(m.dict.table("RSRCRCAT").is_none_or(|t| t.rows.is_empty()));
}

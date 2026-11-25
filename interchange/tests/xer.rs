use openxer_interchange::xer_export::{export_document, header_version};
use openxer_interchange::xer_merge::{merge_project, Action, DataType, ImportConfig};
use openxer_xer::{starter, Document, ProjectData};
use std::path::PathBuf;

fn corpus() -> Option<PathBuf> {
    // The corpus holds client data outside the repository; OPENXER_CORPUS
    // names its folder, and the tests skip without it.
    let Some(dir) = std::env::var_os("OPENXER_CORPUS") else {
        eprintln!("SKIPPED: OPENXER_CORPUS is not set, so this corpus test did not run");
        return None;
    };
    let dir = PathBuf::from(dir);
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!(
            "SKIPPED: corpus directory {} is not available, so this corpus test did not run",
            dir.display()
        );
        None
    }
}

fn load(doc: &Document, pid: i64) -> ProjectData {
    ProjectData::from_document(doc.clone(), pid, "test".into()).unwrap()
}

/// A copy of the starter with one renamed activity, one new activity, a changed lag
/// and a removed relationship, as a contractor's updated XER would carry.
fn edited_starter() -> (ProjectData, Document) {
    let mut p = starter();
    p.details.get_mut(&1010).unwrap().name = "Renamed in the field".into();
    let wbs = p.wbs[1].id;
    let i = p.add_activity(wbs);
    p.model.activities[i].task_code = "NEW-1".into();
    p.model.links[0].lag = 480;
    p.model.links.remove(1);
    p.model.rebuild_links().unwrap();
    let doc = p.updated_document();
    (p, doc)
}

#[test]
fn update_existing_merges_changes_and_reports_counts() {
    let target = starter();
    let (edited, source) = edited_starter();
    let pid = target.model.project.proj_id;
    let (merged, report) = merge_project(
        &target.updated_document(),
        pid,
        &source,
        pid,
        &ImportConfig::default(),
    )
    .unwrap();
    let after = load(&merged, pid);
    let renamed = after
        .model
        .activities
        .iter()
        .find(|a| a.task_code == edited.model.activities[1].task_code)
        .unwrap();
    assert_eq!(after.details[&renamed.task_id].name, "Renamed in the field");
    assert!(after
        .model
        .activities
        .iter()
        .any(|a| a.task_code == "NEW-1"));
    let a = &report.counts[&DataType::Activities];
    assert_eq!(a.added, 1);
    assert_eq!(a.updated, 1, "{report:?}");
    let r = &report.counts[&DataType::Relationships];
    assert_eq!(r.updated, 1);
    assert_eq!(r.deleted, 0, "delete is off by default");
    assert_eq!(after.model.links.len(), target.model.links.len());
    // Calendars matched by name are kept, not duplicated.
    assert_eq!(report.counts[&DataType::Calendars].kept, 1);
    assert_eq!(after.model.calendars.len(), target.model.calendars.len());
}

#[test]
fn delete_flag_removes_relationships_missing_from_the_file() {
    let target = starter();
    let (_, source) = edited_starter();
    let pid = target.model.project.proj_id;
    let mut config = ImportConfig::default();
    config.set(DataType::Relationships, Action::UpdateExisting, true);
    let (merged, report) =
        merge_project(&target.updated_document(), pid, &source, pid, &config).unwrap();
    assert_eq!(report.counts[&DataType::Relationships].deleted, 1);
    assert_eq!(
        load(&merged, pid).model.links.len(),
        target.model.links.len() - 1
    );
}

#[test]
fn keep_existing_and_do_not_import_leave_matches_alone() {
    let target = starter();
    let (_, source) = edited_starter();
    let pid = target.model.project.proj_id;
    let mut config = ImportConfig::default();
    config.set(DataType::Activities, Action::KeepExisting, false);
    config.set(DataType::Relationships, Action::DoNotImport, false);
    let (merged, report) =
        merge_project(&target.updated_document(), pid, &source, pid, &config).unwrap();
    let after = load(&merged, pid);
    assert_eq!(after.details[&1010].name, target.details[&1010].name);
    assert_eq!(report.counts[&DataType::Activities].added, 1);
    assert_eq!(after.model.links, target.model.links);
}

#[test]
fn insert_new_calendar_creates_a_renamed_copy() {
    let target = starter();
    let pid = target.model.project.proj_id;
    let mut config = ImportConfig::default();
    config.set(DataType::Calendars, Action::InsertNew, false);
    let doc = target.updated_document();
    let (merged, report) = merge_project(&doc, pid, &doc, pid, &config).unwrap();
    assert_eq!(report.counts[&DataType::Calendars].added, 1);
    let names: Vec<_> = load(&merged, pid).calendar_names;
    assert!(names.iter().any(|n| n.ends_with("_1")), "{names:?}");
}

#[test]
fn delete_activities_cascades() {
    let target = starter();
    let pid = target.model.project.proj_id;
    let mut source = starter();
    let removed = source
        .model
        .activities
        .remove(source.model.activities.len() - 1);
    source.model.links.clear();
    // Rebuild a document without the activity row.
    let mut doc = source.updated_document();
    openxer_interchange::xer_merge::delete_activities(
        &mut doc,
        pid,
        &[removed.task_id.to_string()],
    );
    let mut config = ImportConfig::default();
    config.set(DataType::Activities, Action::UpdateExisting, true);
    let (merged, report) =
        merge_project(&target.updated_document(), pid, &doc, pid, &config).unwrap();
    assert_eq!(report.counts[&DataType::Activities].deleted, 1);
    let after = load(&merged, pid);
    assert!(!after
        .model
        .activities
        .iter()
        .any(|a| a.task_id == removed.task_id));
    assert!(!merged
        .table("TASKPRED")
        .unwrap()
        .rows
        .iter()
        .any(|r| r.iter().any(|v| *v == removed.task_id.to_string())));
}

#[test]
fn config_parse_round_trip() {
    let c = ImportConfig::parse("activities=update+delete,calendars=insert,wbs=keep").unwrap();
    assert!(c.get(DataType::Activities).delete);
    assert_eq!(c.get(DataType::Calendars).action, Action::InsertNew);
    assert_eq!(c.get(DataType::Wbs).action, Action::KeepExisting);
    assert!(ImportConfig::parse("wbs=insert").is_err());
    assert!(ImportConfig::parse("calendars=keep+delete").is_err());
    assert!(ImportConfig::parse("bogus=keep").is_err());
}

#[test]
fn starter_export_reimports_exactly() {
    let s = starter();
    let p = load(&s.updated_document(), s.model.project.proj_id);
    let doc = export_document(&[&p], "19.12").unwrap();
    assert_eq!(header_version(&doc.header), "19.12");
    let text = doc.text();
    let back = Document::parse(&text).unwrap();
    let again = load(&back, p.model.project.proj_id);
    assert!(again.model == p.model, "model changed on reimport");
    assert!(again.details == p.details, "details changed on reimport");
    // Everything after the header is identical to the project's own document.
    let own = p.updated_document().text();
    assert_eq!(
        text.split_once("\r\n").unwrap().1,
        own.split_once("\r\n").unwrap().1
    );
}

#[test]
fn corpus_self_update_is_a_no_op_and_export_round_trips() {
    let Some(dir) = corpus() else { return };
    let mut checked = 0;
    let mut names: Vec<String> = vec!["5932.xer".into(), "6031.xer".into()];
    // A third file's name identifies its job, so it comes from the private
    // fixture `corpus-names.txt` (`self-update <file>`); without it the two
    // files above still run and a SKIPPED line says what did not.
    if let Some(listed) = openxer_engine::fixture::corpus_names("self-update") {
        names.extend(listed.into_iter().map(|(file, _)| file));
    }
    for name in &names {
        let name = name.as_str();
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("SKIPPED: {name} is not in the corpus");
            continue;
        }
        let doc = Document::read(&path).unwrap();
        for (pid, _) in doc.projects() {
            let Ok(p) = ProjectData::from_document(doc.clone(), pid, name.into()) else {
                continue;
            };
            let base = p.updated_document();
            let (merged, report) =
                merge_project(&base, pid, &base, pid, &ImportConfig::default()).unwrap();
            for (t, c) in &report.counts {
                assert_eq!(c.added, 0, "{name} {t:?} {c:?}");
                assert_eq!(c.updated, 0, "{name} {t:?} {c:?}");
            }
            let again = load(&merged, pid);
            assert_eq!(again.model, p.model, "{name}");
            // Export then reimport is exact.
            let exported = export_document(&[&p], "25.12").unwrap();
            let back = load(&Document::parse(&exported.text()).unwrap(), pid);
            assert_eq!(back.model, p.model, "{name}");
            assert_eq!(back.details, p.details, "{name}");
            checked += 1;
        }
    }
    eprintln!("checked {checked} corpus projects");
}

#[test]
fn corpus_update_from_edited_copy() {
    let Some(dir) = corpus() else { return };
    let path = dir.join("5932.xer");
    if !path.exists() {
        eprintln!("SKIPPED: 5932.xer is not in the corpus");
        return;
    }
    let doc = Document::read(&path).unwrap();
    let pid = doc.projects()[0].0;
    let p = load(&doc, pid);
    let mut edited = p.clone();
    let id = edited.model.activities[3].task_id;
    edited.details.get_mut(&id).unwrap().name = "Edited by contractor".into();
    edited.model.activities[3].rem += 60;
    let (merged, report) = merge_project(
        &p.updated_document(),
        pid,
        &edited.updated_document(),
        pid,
        &ImportConfig::default(),
    )
    .unwrap();
    assert_eq!(report.counts[&DataType::Activities].updated, 1);
    let after = load(&merged, pid);
    assert_eq!(after.details[&id].name, "Edited by contractor");
    assert_eq!(
        after.model.activities[3].rem,
        p.model.activities[3].rem + 60
    );
}

#[test]
fn multi_project_export_keeps_each_project() {
    let Some(dir) = corpus() else { return };
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "xer") {
            continue;
        }
        let doc = Document::read(&path).unwrap();
        let projects: Vec<ProjectData> = doc
            .projects()
            .into_iter()
            .filter_map(|(pid, _)| ProjectData::from_document(doc.clone(), pid, "x".into()).ok())
            .collect();
        if projects.len() < 2 {
            continue;
        }
        let refs: Vec<&ProjectData> = projects.iter().take(2).collect();
        let out = export_document(&refs, "24.12").unwrap();
        let back = Document::parse(&out.text()).unwrap();
        assert_eq!(back.projects().len(), 2);
        for p in &refs {
            let again = load(&back, p.model.project.proj_id);
            assert_eq!(again.model, p.model);
        }
        eprintln!("multi-project export checked on {}", path.display());
        return;
    }
    eprintln!("SKIPPED: no multi-project XER in the corpus");
}

/// Two projects of one file, opened and edited separately, export together:
/// each keeps its own rows (not the stale copy stored with the other) and the
/// cross-project relationship is written once.
#[test]
fn same_file_projects_export_together_with_their_cross_project_relationship() {
    let doc = openxer_xer::starter_portfolio();
    let rb = load(&doc, 1);
    let mut ny = load(&doc, 2);
    ny.details.get_mut(&2010).unwrap().name = "Renamed in NORTHYARD".into();
    ny.add_external_link(1, 1130, 2130, openxer_engine::model::LinkType::FF, 480)
        .unwrap();
    // A new relationship in RIVERBEND takes the same next row id as the one
    // NORTHYARD just added in its own copy of the file.
    let mut rb = rb;
    rb.model.links.push(openxer_engine::model::Link {
        pred: 0,
        succ: 13,
        link_type: openxer_engine::model::LinkType::FS,
        lag: 0,
        driving_path: false,
    });
    rb.model.rebuild_links().unwrap();
    for order in [[&rb, &ny], [&ny, &rb]] {
        let out = export_document(&order, "25.12").unwrap();
        let back = Document::parse(&out.text()).unwrap();
        assert_eq!(back.projects().len(), 2);
        let ids: Vec<String> = back
            .table("TASKPRED")
            .unwrap()
            .rows
            .iter()
            .map(|r| r[0].clone())
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "relationship ids are unique");
        for p in [&rb, &ny] {
            let again = load(&back, p.model.project.proj_id);
            assert_eq!(again.model, p.model);
            assert_eq!(again.details, p.details);
            let mut a = again.external_links();
            let mut b = p.external_links();
            for l in a.iter_mut().chain(b.iter_mut()) {
                l.row_id = 0;
            }
            assert_eq!(a, b);
        }
        assert_eq!(load(&back, 2).external_links().len(), 2);
    }
}

/// The two-project sample with one UDF value, one period performance row and
/// one assignment period row in each project. UDFVALUE, TASKFIN and TRSRCFIN
/// rows share their first column (`udf_type_id`, `fin_dates_id`) across
/// projects, so that column cannot identify them.
fn portfolio_with_shared_first_columns() -> Document {
    let mut doc = openxer_xer::starter_portfolio();
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "900".into()),
        ("table_name", "TASK".into()),
        ("udf_type_name", "user_field_900".into()),
        ("udf_type_label", "Foreman".into()),
        ("logical_data_type", "FT_TEXT".into()),
    ]);
    doc.table_mut("FINDATES").add(&[
        ("fin_dates_id", "5".into()),
        ("fin_dates_name", "Period 1".into()),
        ("start_date", "2026-11-01 00:00".into()),
        ("end_date", "2026-11-30 00:00".into()),
    ]);
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("rsrc_short_name", "CREW".into()),
        ("rsrc_name", "Field crew".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("clndr_id", "1".into()),
    ]);
    for (pid, task, assignment, foreman) in [(1, 1010, 501, "A. North"), (2, 2010, 502, "B. South")]
    {
        doc.table_mut("UDFVALUE").add(&[
            ("udf_type_id", "900".into()),
            ("fk_id", task.to_string()),
            ("proj_id", pid.to_string()),
            ("udf_text", foreman.into()),
        ]);
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", assignment.to_string()),
            ("task_id", task.to_string()),
            ("proj_id", pid.to_string()),
            ("rsrc_id", "10".into()),
            ("target_qty", "16".into()),
            ("remain_qty", "16".into()),
        ]);
        doc.table_mut("TASKFIN").add(&[
            ("fin_dates_id", "5".into()),
            ("task_id", task.to_string()),
            ("proj_id", pid.to_string()),
            ("act_work_qty", "4".into()),
        ]);
        doc.table_mut("TRSRCFIN").add(&[
            ("fin_dates_id", "5".into()),
            ("taskrsrc_id", assignment.to_string()),
            ("task_id", task.to_string()),
            ("proj_id", pid.to_string()),
            ("act_reg_qty", "4".into()),
        ]);
    }
    doc
}

/// Regression: multi-project export treated each table's first column as its
/// key, so the second project's UDF values and period rows were dropped as
/// "dictionary rows already present".
#[test]
fn multi_project_export_keeps_rows_whose_first_column_is_shared() {
    let doc = portfolio_with_shared_first_columns();
    let rb = load(&doc, 1);
    let ny = load(&doc, 2);
    for order in [[&rb, &ny], [&ny, &rb]] {
        let out = export_document(&order, "25.12").unwrap();
        let back = Document::parse(&out.text()).unwrap();
        for (table, column, wanted) in [
            ("UDFVALUE", "fk_id", ["1010", "2010"]),
            ("TASKFIN", "task_id", ["1010", "2010"]),
            ("TRSRCFIN", "taskrsrc_id", ["501", "502"]),
        ] {
            let t = back.table(table).unwrap();
            let mut got: Vec<&str> = t.rows.iter().map(|r| t.value(r, column)).collect();
            got.sort();
            assert_eq!(got, wanted, "{table}");
        }
        let udf = back.table("UDFVALUE").unwrap();
        assert!(udf
            .rows
            .iter()
            .any(|r| udf.value(r, "udf_text") == "B. South"));
    }
}

/// Projects from different files that both name calendar 1 but mean
/// different work weeks are refused: reusing either calendar would move the
/// other project's dates.
#[test]
fn multi_project_export_refuses_a_shared_calendar_id_with_other_content() {
    let doc = openxer_xer::starter_portfolio();
    let mut other = doc.clone();
    {
        let t = other.table_mut("CALENDAR");
        let data = t
            .value(&t.rows[0], "clndr_data")
            .replace("(0||7()())", "(0||7()((0||0(s|08:00|f|12:00)())))");
        assert_ne!(data, t.value(&t.rows[0], "clndr_data"));
        t.set(0, "clndr_data", data);
    }
    let rb = load(&doc, 1);
    let ny = load(&other, 2);
    let Err(err) = export_document(&[&rb, &ny], "25.12") else {
        panic!("a calendar with other work times is refused");
    };
    assert!(err.contains("calendar"), "{err}");
    // The same calendar in both is not a conflict.
    let same = load(&doc, 2);
    assert!(export_document(&[&rb, &same], "25.12").is_ok());
}

/// Add Into Existing Project: the file's activities join the project's own;
/// clashing activity IDs get fresh ones, nothing existing changes, and the
/// imported relationships come along between the new activities.
#[test]
fn add_into_existing_project_appends_with_fresh_ids() {
    use openxer_interchange::xer_merge::add_into_project;
    let target = starter();
    let pid = target.model.project.proj_id;
    let source = target.updated_document();
    let (merged, report) =
        add_into_project(&source, pid, &source, pid, &ImportConfig::default()).unwrap();
    let p = load(&merged, pid);
    let n = target.model.activities.len();
    assert_eq!(p.model.activities.len(), 2 * n);
    assert_eq!(p.model.links.len(), 2 * target.model.links.len());
    // The original activities are untouched and keep their IDs.
    for a in &target.model.activities {
        let b = p
            .model
            .activities
            .iter()
            .find(|b| b.task_id == a.task_id)
            .unwrap();
        assert_eq!(a.task_code, b.task_code);
    }
    let codes: std::collections::HashSet<&str> = p
        .model
        .activities
        .iter()
        .map(|a| a.task_code.as_str())
        .collect();
    assert_eq!(codes.len(), 2 * n, "every activity ID is unique");
    let first = &target.model.activities[0].task_code;
    assert!(codes.contains(format!("{first}-1").as_str()));
    assert!(report
        .messages
        .iter()
        .any(|m| m.contains(&format!("{n} activities took a new activity ID"))));
    assert_eq!(
        p.model.project.data_date, target.model.project.data_date,
        "the project's own settings stay"
    );
    // WBS elements match by code: no new elements.
    assert_eq!(p.wbs.len(), target.wbs.len());
    // A file whose IDs are free keeps them.
    let (edited, other) = edited_starter();
    let mut lone = edited.clone();
    let keep: Vec<i64> = lone
        .model
        .activities
        .iter()
        .filter(|a| a.task_code == "NEW-1")
        .map(|a| a.task_id)
        .collect();
    let drop: Vec<i64> = lone
        .model
        .activities
        .iter()
        .map(|a| a.task_id)
        .filter(|id| !keep.contains(id))
        .collect();
    lone.delete_activities(&drop).unwrap();
    let (merged, report) = add_into_project(
        &target.updated_document(),
        pid,
        &lone.updated_document(),
        pid,
        &ImportConfig::default(),
    )
    .unwrap();
    let p = load(&merged, pid);
    assert_eq!(p.model.activities.len(), n + 1);
    assert!(p.model.activities.iter().any(|a| a.task_code == "NEW-1"));
    assert!(report
        .messages
        .iter()
        .all(|m| !m.contains("new activity ID")));
    let _ = other;
}

fn add(doc: &mut Document, table: &str, values: &[(&str, &str)]) {
    let values: Vec<(&str, String)> = values.iter().map(|(k, v)| (*k, v.to_string())).collect();
    doc.table_mut(table).add(&values);
}

/// Regression: merge remapped activity, resource, project, WBS and calendar
/// ids but copied role, cost account, curve, cost type, user and shift ids
/// as they were, so they named unrelated rows of the target, and resource
/// rates were not merged at all.
#[test]
fn merge_maps_roles_accounts_curves_cost_types_shifts_and_rates() {
    let target = starter();
    let pid = target.model.project.proj_id;
    let mut tdoc = target.updated_document();
    add(
        &mut tdoc,
        "ROLES",
        &[
            ("role_id", "5"),
            ("role_short_name", "ENG"),
            ("role_name", "Engineer"),
        ],
    );
    add(
        &mut tdoc,
        "ROLES",
        &[
            ("role_id", "15"),
            ("role_short_name", "OPS"),
            ("role_name", "Operator"),
        ],
    );
    add(
        &mut tdoc,
        "ACCOUNT",
        &[
            ("acct_id", "50"),
            ("acct_short_name", "A100"),
            ("acct_name", "Labor"),
        ],
    );
    add(
        &mut tdoc,
        "ACCOUNT",
        &[
            ("acct_id", "150"),
            ("acct_short_name", "Z900"),
            ("acct_name", "Other"),
        ],
    );
    add(
        &mut tdoc,
        "RSRCCURVDATA",
        &[("curv_id", "60"), ("curv_name", "Front loaded")],
    );
    add(
        &mut tdoc,
        "COSTTYPE",
        &[("cost_type_id", "70"), ("cost_type", "Material")],
    );
    add(
        &mut tdoc,
        "SHIFT",
        &[("shift_id", "80"), ("shift_name", "Day")],
    );
    add(
        &mut tdoc,
        "SHIFTPER",
        &[
            ("shift_period_id", "81"),
            ("shift_id", "80"),
            ("shift_start_hr_num", "0"),
        ],
    );

    let mut sdoc = starter().updated_document();
    add(
        &mut sdoc,
        "ROLES",
        &[
            ("role_id", "15"),
            ("role_short_name", "ENG"),
            ("role_name", "Engineer"),
        ],
    );
    add(
        &mut sdoc,
        "ACCOUNT",
        &[
            ("acct_id", "150"),
            ("acct_short_name", "A100"),
            ("acct_name", "Labor"),
        ],
    );
    add(
        &mut sdoc,
        "ACCOUNT",
        &[
            ("acct_id", "151"),
            ("acct_short_name", "B200"),
            ("acct_name", "Hoisting"),
            ("parent_acct_id", "150"),
        ],
    );
    add(
        &mut sdoc,
        "RSRCCURVDATA",
        &[("curv_id", "160"), ("curv_name", "Front loaded")],
    );
    add(
        &mut sdoc,
        "COSTTYPE",
        &[("cost_type_id", "170"), ("cost_type", "Material")],
    );
    add(
        &mut sdoc,
        "SHIFT",
        &[("shift_id", "25"), ("shift_name", "Day")],
    );
    add(
        &mut sdoc,
        "SHIFTPER",
        &[
            ("shift_period_id", "26"),
            ("shift_id", "25"),
            ("shift_start_hr_num", "0"),
        ],
    );
    add(
        &mut sdoc,
        "RSRC",
        &[
            ("rsrc_id", "20"),
            ("rsrc_short_name", "CRANE"),
            ("rsrc_name", "Crane crew"),
            ("rsrc_type", "RT_Labor"),
            ("clndr_id", "1"),
            ("role_id", "15"),
            ("shift_id", "25"),
            ("user_id", "30"),
        ],
    );
    add(
        &mut sdoc,
        "RSRCRATE",
        &[
            ("rsrc_rate_id", "9"),
            ("rsrc_id", "20"),
            ("cost_per_qty", "120"),
            ("shift_period_id", "26"),
        ],
    );
    add(
        &mut sdoc,
        "TASKRSRC",
        &[
            ("taskrsrc_id", "700"),
            ("task_id", "1010"),
            ("proj_id", "1"),
            ("rsrc_id", "20"),
            ("role_id", "15"),
            ("acct_id", "151"),
            ("curv_id", "160"),
            ("target_qty", "8"),
            ("remain_qty", "8"),
        ],
    );
    add(
        &mut sdoc,
        "PROJCOST",
        &[
            ("cost_item_id", "800"),
            ("task_id", "1010"),
            ("proj_id", "1"),
            ("acct_id", "150"),
            ("cost_type_id", "170"),
            ("target_cost", "500"),
            ("cost_name", "Rigging"),
        ],
    );

    let (merged, report) = merge_project(&tdoc, pid, &sdoc, pid, &ImportConfig::default()).unwrap();
    let row = |table: &str, key: &str, value: &str| -> Vec<String> {
        let t = merged.table(table).unwrap();
        let r = t
            .rows
            .iter()
            .find(|r| t.value(r, key) == value)
            .unwrap_or_else(|| panic!("{table} {key}={value}"));
        r.clone()
    };
    let get = |table: &str, r: &[String], field: &str| {
        merged.table(table).unwrap().value(r, field).to_string()
    };
    let crane = row("RSRC", "rsrc_short_name", "CRANE");
    let crane_id = get("RSRC", &crane, "rsrc_id");
    assert_eq!(get("RSRC", &crane, "role_id"), "5", "{report:?}");
    assert_eq!(get("RSRC", &crane, "shift_id"), "80");
    assert_eq!(
        get("RSRC", &crane, "user_id"),
        "",
        "no such user in the target"
    );
    let rate = row("RSRCRATE", "rsrc_id", &crane_id);
    assert_eq!(get("RSRCRATE", &rate, "cost_per_qty"), "120");
    assert_eq!(get("RSRCRATE", &rate, "shift_period_id"), "81");
    let hoisting = row("ACCOUNT", "acct_short_name", "B200");
    assert_eq!(get("ACCOUNT", &hoisting, "parent_acct_id"), "50");
    let assignment = row("TASKRSRC", "rsrc_id", &crane_id);
    assert_eq!(get("TASKRSRC", &assignment, "role_id"), "5");
    assert_eq!(
        get("TASKRSRC", &assignment, "acct_id"),
        get("ACCOUNT", &hoisting, "acct_id")
    );
    assert_eq!(get("TASKRSRC", &assignment, "curv_id"), "60");
    let expense = row("PROJCOST", "cost_name", "Rigging");
    assert_eq!(get("PROJCOST", &expense, "acct_id"), "50");
    assert_eq!(get("PROJCOST", &expense, "cost_type_id"), "70");
    assert_eq!(
        merged.table("ROLES").unwrap().rows.len(),
        2,
        "matched, not copied"
    );
}

/// The starter with `n` more activities in a chain, ids from 100000,
/// Activity IDs `S-000000` on.
fn large_schedule(n: usize, prefix: &str) -> Document {
    let mut doc = starter().updated_document();
    for i in 0..n {
        let id = (100_000 + i).to_string();
        doc.table_mut("TASK").add(&[
            ("task_id", id),
            ("proj_id", "1".into()),
            ("wbs_id", "30".into()),
            ("clndr_id", "1".into()),
            ("task_code", format!("{prefix}-{i:06}")),
            ("task_name", format!("Synthetic activity {i}")),
            ("task_type", "TT_Task".into()),
            ("status_code", "TK_NotStart".into()),
            ("duration_type", "DT_FixedDUR2".into()),
            ("remain_drtn_hr_cnt", "8".into()),
            ("target_drtn_hr_cnt", "8".into()),
        ]);
        if i > 0 {
            doc.table_mut("TASKPRED").add(&[
                ("task_pred_id", (100_000 + i).to_string()),
                ("task_id", (100_000 + i).to_string()),
                ("pred_task_id", (100_000 + i - 1).to_string()),
                ("proj_id", "1".into()),
                ("pred_proj_id", "1".into()),
                ("pred_type", "PR_FS".into()),
                ("lag_hr_cnt", "0".into()),
            ]);
        }
    }
    doc
}

/// Merging tens of thousands of activities took one scan of the target per
/// file row (5,000 took 7 s in a debug build, so 20,000 about two minutes);
/// now 20,000 take about 10 s, most of it loading the project twice. The
/// bound is generous for a busy machine; the point is that the work grows
/// with the rows, not with their square.
#[test]
fn merging_a_large_schedule_is_not_quadratic() {
    let n: usize = std::env::var("OPENXER_MERGE_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let target = large_schedule(n, "S");
    let mut source = large_schedule(n, "S");
    {
        let t = source.table_mut("TASK");
        let name = t.field("task_name").unwrap();
        for r in t.rows.iter_mut().skip(14).step_by(2) {
            r[name] = format!("{} (revised)", r[name]);
        }
    }
    let started = std::time::Instant::now();
    let (merged, report) = merge_project(&target, 1, &source, 1, &ImportConfig::default()).unwrap();
    let took = started.elapsed();
    eprintln!("merged {n} activities in {took:?}");
    assert_eq!(report.counts[&DataType::Activities].updated, n / 2);
    assert_eq!(
        merged.table("TASK").unwrap().rows.len(),
        target.table("TASK").unwrap().rows.len()
    );
    assert!(took.as_secs() < 60, "{took:?}");
}

use super::*;
use crate::edit::records;
use std::path::PathBuf;

/// Calendar data for a five-day, eight-hour week.
pub(crate) fn week() -> String {
    let days = (1..=7)
        .map(|d| {
            if (2..=6).contains(&d) {
                format!("(0||{d}()((0||0(s|08:00|f|16:00)())))")
            } else {
                format!("(0||{d}()())")
            }
        })
        .collect::<String>();
    format!("(0||CalendarData()((0||DaysOfWeek()({days}))(0||Exceptions()())))")
}

/// One XER text from (table, fields, rows) with tab-separated rows.
pub(crate) fn xer(tables: &[(&str, &str, &[&str])]) -> Document {
    let mut s = String::from(
        "ERMHDR\t25.12\t2026-11-08\tProject\tOpenXER\tOpenXER\tdb\tProject Management\tUSD\r\n",
    );
    for (name, fields, rows) in tables {
        s.push_str(&format!(
            "%T\t{name}\r\n%F\t{}\r\n",
            fields.replace(' ', "\t")
        ));
        for r in *rows {
            let row = r.replace('|', "\t").replace("@WEEK", &week());
            s.push_str(&format!("%R\t{row}\r\n"));
        }
    }
    s.push_str("%E\r\n");
    Document::parse(&s).unwrap()
}

/// A synthetic project with workspace and project rows interleaved in the
/// tables that hold both, references of every kind, and a UDF on resources.
pub(crate) fn synthetic(pid: i64, name: &str) -> Document {
    let p = pid.to_string();
    let project = format!("{p}|{name}|2026-11-02 08:00|2026-11-02 08:00|2");
    let task1 = format!("101|{p}|11|3|7|A100|Dig|TT_Task|TK_NotStart|16|16");
    let task2 = format!("102|{p}|11|2||A110|Pour|TT_Task|TK_NotStart|8|8");
    let wbs = format!("11|{p}|Y|{name}|{name}||5");
    let cal_project = format!("3|N|Night {name}|{p}|2|CA_Project|@WEEK");
    let tr = format!("201|101|{p}|7|8|16|31");
    let ta1 = format!("101|{p}|40|41");
    let ta2 = format!("102|{p}|40|42");
    let ta3 = format!("101|{p}|45|46");
    let uv = format!("7|7|{p}||Crew A");
    let uv2 = format!("8|101|{p}||Note");
    let at_project = format!("45|5|2|Area|{p}||AS_Project|N");
    let rows: Vec<(&str, &str, Vec<String>)> = vec![
        (
            "CURRTYPE",
            "curr_id curr_short_name curr_symbol",
            vec!["1|USD|$".into()],
        ),
        (
            "OBS",
            "obs_id parent_obs_id guid seq_num obs_name",
            vec!["5||g-obs|0|Enterprise".into()],
        ),
        (
            "UDFTYPE",
            "udf_type_id table_name udf_type_name udf_type_label logical_data_type",
            vec![
                "7|RSRC|user_field_1|Crew|FT_TEXT".into(),
                "8|TASK|user_field_2|Note|FT_TEXT".into(),
            ],
        ),
        (
            "PROJECT",
            "proj_id proj_short_name last_recalc_date plan_start_date clndr_id",
            vec![project],
        ),
        (
            "CALENDAR",
            "clndr_id default_flag clndr_name proj_id base_clndr_id clndr_type clndr_data",
            vec![
                "1|N|Base 24|||CA_Base|@WEEK".into(),
                "2|Y|Standard|||CA_Base|@WEEK".into(),
                cal_project,
                "4|N|Crew cal|||CA_Rsrc|@WEEK".into(),
            ],
        ),
        (
            "PROJWBS",
            "wbs_id proj_id proj_node_flag wbs_short_name wbs_name parent_wbs_id obs_id",
            vec![wbs],
        ),
        (
            "RSRC",
            "rsrc_id parent_rsrc_id clndr_id rsrc_name rsrc_short_name curr_id guid",
            vec!["7||4|Operator|OP|1|g-op".into(), "8|7|2|Helper|HLP|1|g-hlp".into()],
        ),
        (
            "ACTVTYPE",
            "actv_code_type_id actv_short_len seq_num actv_code_type proj_id wbs_id actv_code_type_scope export_flag",
            vec!["40|5|1|Phase|||AS_Global|Y".into(), at_project],
        ),
        (
            "RSRCRATE",
            "rsrc_rate_id rsrc_id max_qty_per_hr cost_per_qty start_date",
            vec!["31|7|1|50|2020-01-01 00:00".into(), "32|8|2|30|2020-01-01 00:00".into()],
        ),
        (
            "TASK",
            "task_id proj_id wbs_id clndr_id rsrc_id task_code task_name task_type status_code remain_drtn_hr_cnt target_drtn_hr_cnt",
            vec![task1, task2],
        ),
        (
            "ACTVCODE",
            "actv_code_id parent_actv_code_id actv_code_type_id actv_code_name short_name seq_num total_assignments",
            vec![
                "41||40|Civil|CIV|1|1".into(),
                "42|41|40|Concrete|CON|2|1".into(),
                "46||45|North|N|1|1".into(),
            ],
        ),
        (
            "TASKRSRC",
            "taskrsrc_id task_id proj_id rsrc_id role_id remain_qty target_qty",
            vec![tr],
        ),
        (
            "TASKACTV",
            "task_id proj_id actv_code_type_id actv_code_id",
            vec![ta1, ta2, ta3],
        ),
        (
            "UDFVALUE",
            "udf_type_id fk_id proj_id udf_date udf_text",
            vec![uv, uv2],
        ),
    ];
    let borrowed: Vec<Vec<&str>> = rows
        .iter()
        .map(|(_, _, r)| r.iter().map(String::as_str).collect())
        .collect();
    let tables: Vec<(&str, &str, &[&str])> = rows
        .iter()
        .zip(&borrowed)
        .map(|((n, f, _), r)| (*n, *f, r.as_slice()))
        .collect();
    xer(&tables)
}

pub(crate) fn corpus() -> Option<Vec<PathBuf>> {
    let dir = std::env::var_os("OPENXER_CORPUS").map(PathBuf::from);
    let Some(entries) = dir.as_ref().and_then(|d| std::fs::read_dir(d).ok()) else {
        println!("SKIPPED: XER corpus not found (set OPENXER_CORPUS)");
        return None;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xer")))
        .collect();
    files.sort();
    (!files.is_empty()).then_some(files)
}

fn rows_of(doc: &Document, table: &str) -> usize {
    doc.table(table).map_or(0, |t| t.rows.len())
}

#[test]
fn scopes_follow_calendar_type_and_code_type_scope() {
    let doc = synthetic(1, "ALPHA");
    let s = Scopes::of(&doc);
    let cal = doc.table("CALENDAR").unwrap();
    let kinds: Vec<Scope> = cal.rows.iter().map(|r| s.scope(cal, r)).collect();
    assert_eq!(
        kinds,
        [
            Scope::Workspace,
            Scope::Workspace,
            Scope::Project,
            Scope::Workspace
        ]
    );
    let codes = doc.table("ACTVCODE").unwrap();
    let kinds: Vec<Scope> = codes.rows.iter().map(|r| s.scope(codes, r)).collect();
    assert_eq!(kinds, [Scope::Workspace, Scope::Workspace, Scope::Project]);
    let task = doc.table("TASK").unwrap();
    assert_eq!(s.scope(task, &task.rows[0]), Scope::Project);
    let rsrc = doc.table("RSRC").unwrap();
    assert_eq!(scope(&doc, rsrc, &rsrc.rows[0]), Scope::Workspace);
    let eps = xer(&[(
        "ACTVTYPE",
        "actv_code_type_id actv_code_type proj_id wbs_id actv_code_type_scope",
        &["9|Region||77|AS_EPS"],
    )]);
    let t = eps.table("ACTVTYPE").unwrap();
    assert_eq!(scope(&eps, t, &t.rows[0]), Scope::Eps(77));
}

#[test]
fn split_moves_workspace_rows_and_keeps_project_rows() {
    let doc = synthetic(1, "ALPHA");
    let (project, workspace) = split(&doc);
    assert_eq!(rows_of(&project, "CALENDAR"), 1);
    assert_eq!(rows_of(&workspace, "CALENDAR"), 3);
    assert_eq!(rows_of(&project, "RSRC"), 0);
    assert!(
        project.table("RSRC").is_some(),
        "the emptied table keeps its place"
    );
    assert_eq!(rows_of(&workspace, "RSRC"), 2);
    assert_eq!(rows_of(&workspace, "ACTVTYPE"), 1);
    assert_eq!(rows_of(&project, "ACTVTYPE"), 1);
    assert_eq!(rows_of(&workspace, "ACTVCODE"), 2);
    assert_eq!(rows_of(&project, "TASK"), 2);
    assert!(workspace.table("TASK").is_none());
    assert_eq!(rows_of(&project, "UDFVALUE"), 2);
    assert_eq!(rows_of(&workspace, "UDFTYPE"), 2);
}

#[test]
fn split_then_hydrate_restores_the_synthetic_document_exactly() {
    for doc in [synthetic(1, "ALPHA"), synthetic(9, "OMEGA")] {
        let (mut project, workspace) = split(&doc);
        let report = hydrate(&mut project, &workspace);
        assert!(report.shadowed.is_empty());
        assert_eq!(project.text(), doc.text());
        assert_eq!(project.lone_quotes, doc.lone_quotes);
        // Through stored text, as the workspace keeps them.
        let mut stored = Document::parse(&split(&doc).0.text()).unwrap();
        let dict = Document::parse(&workspace.text()).unwrap();
        hydrate(&mut stored, &dict);
        assert_eq!(stored.text(), doc.text());
    }
}

#[test]
fn hydrate_into_a_project_without_the_tables_adds_them_in_export_order() {
    let doc = synthetic(1, "ALPHA");
    let (_, workspace) = split(&doc);
    let mut other = xer(&[
        (
            "PROJECT",
            "proj_id proj_short_name last_recalc_date clndr_id",
            &["5|BETA|2026-11-02 08:00|2"],
        ),
        ("TASK", "task_id proj_id clndr_id", &["900|5|2"]),
    ]);
    let report = hydrate(&mut other, &workspace);
    assert!(report.added.contains(&("CALENDAR".into(), 2)));
    assert!(report.added.contains(&("RSRC".into(), 7)));
    let names: Vec<&str> = other.tables.iter().map(|t| t.name.as_str()).collect();
    let calendar = names.iter().position(|n| *n == "CALENDAR").unwrap();
    let task = names.iter().position(|n| *n == "TASK").unwrap();
    let currency = names.iter().position(|n| *n == "CURRTYPE").unwrap();
    assert!(currency < calendar && calendar < task, "{names:?}");
    assert_eq!(
        other.table("CALENDAR").unwrap().fields,
        workspace.table("CALENDAR").unwrap().fields
    );
}

#[test]
fn hydrate_keeps_the_projects_row_on_an_id_clash_and_adds_missing_columns() {
    let (_, workspace) = split(&synthetic(1, "ALPHA"));
    let mut other = xer(&[(
        "CALENDAR",
        "clndr_id clndr_name clndr_type",
        &["2|Mine|CA_Project", "3|Other|CA_Project"],
    )]);
    let report = hydrate(&mut other, &workspace);
    assert_eq!(report.shadowed, [("CALENDAR".to_string(), 2)]);
    let t = other.table("CALENDAR").unwrap();
    let ids: Vec<&str> = t.rows.iter().map(|r| t.value(r, "clndr_id")).collect();
    assert_eq!(ids, ["1", "2", "3", "4"], "rows go where their ids sort");
    assert_eq!(t.value(&t.rows[1], "clndr_name"), "Mine");
    assert_eq!(&t.fields[..3], ["clndr_id", "clndr_name", "clndr_type"]);
    assert!(t.field("clndr_data").is_some());
    assert_eq!(t.value(&t.rows[2], "clndr_data"), "");
}

#[test]
fn hydrate_twice_adds_nothing_the_second_time() {
    let doc = synthetic(1, "ALPHA");
    let (_, workspace) = split(&doc);
    let mut again = doc.clone();
    let report = hydrate(&mut again, &workspace);
    assert!(report.added.is_empty() && report.added_unkeyed == 0);
    assert!(report.shadowed.is_empty());
    assert_eq!(again.text(), doc.text());
}

#[test]
fn remap_rewrites_ids_and_every_reference_at_once() {
    let mut doc = synthetic(1, "ALPHA");
    let mut map = Remap::default();
    map.insert("CALENDAR", 2, 4);
    map.insert("CALENDAR", 4, 500);
    map.insert("RSRC", 7, 70);
    map.insert("ACTVCODE", 41, 410);
    map.insert("UDFTYPE", 7, 77);
    remap(&mut doc, &map);
    let v = |t: &str, row: usize, col: &str| {
        let t = doc.table(t).unwrap();
        t.value(&t.rows[row], col).to_string()
    };
    assert_eq!(v("CALENDAR", 1, "clndr_id"), "4");
    assert_eq!(v("CALENDAR", 3, "clndr_id"), "500");
    assert_eq!(v("CALENDAR", 2, "base_clndr_id"), "4");
    assert_eq!(v("PROJECT", 0, "clndr_id"), "4");
    assert_eq!(v("TASK", 1, "clndr_id"), "4");
    assert_eq!(v("TASK", 0, "clndr_id"), "3", "project calendars stay");
    assert_eq!(v("RSRC", 0, "rsrc_id"), "70");
    assert_eq!(v("RSRC", 0, "clndr_id"), "500");
    assert_eq!(v("RSRC", 1, "parent_rsrc_id"), "70");
    assert_eq!(v("RSRCRATE", 0, "rsrc_id"), "70");
    assert_eq!(v("TASK", 0, "rsrc_id"), "70");
    assert_eq!(v("TASKRSRC", 0, "rsrc_id"), "70");
    assert_eq!(v("ACTVCODE", 1, "parent_actv_code_id"), "410");
    assert_eq!(v("TASKACTV", 0, "actv_code_id"), "410");
    assert_eq!(v("UDFVALUE", 0, "udf_type_id"), "77");
    assert_eq!(
        v("UDFVALUE", 0, "fk_id"),
        "70",
        "a resource UDF value follows its resource"
    );
    assert_eq!(
        v("UDFVALUE", 1, "fk_id"),
        "101",
        "an activity UDF value does not"
    );
    remap_ids(&mut doc, "RSRC", 70, 7);
    let t = doc.table("TASKRSRC").unwrap();
    assert_eq!(t.value(&t.rows[0], "rsrc_id"), "7");
}

#[test]
fn remap_reaches_risk_issue_and_baseline_type_columns() {
    let mut doc = xer(&[
        (
            "PROJECT",
            "proj_id proj_short_name base_type_id",
            &["1|ALPHA|3"],
        ),
        (
            "PROJRISK",
            "risk_id proj_id risk_type_id rsrc_id",
            &["1|1|5|100"],
        ),
        (
            "RISK",
            "risk_id proj_id risk_type_id rsrc_id",
            &["2|1|5|100"],
        ),
        (
            "PROJISSU",
            "issue_id proj_id rsrc_id thresh_parm_id",
            &["1|1|100|8"],
        ),
    ]);
    let mut map = Remap::default();
    map.insert("RSRC", 100, 250);
    map.insert("RISKTYPE", 5, 50);
    map.insert("THRSPARM", 8, 80);
    map.insert("BASETYPE", 3, 30);
    remap(&mut doc, &map);
    let v = |t: &str, col: &str| {
        let t = doc.table(t).unwrap();
        t.value(&t.rows[0], col).to_string()
    };
    assert_eq!(v("PROJECT", "base_type_id"), "30");
    assert_eq!(v("PROJRISK", "rsrc_id"), "250");
    assert_eq!(v("PROJRISK", "risk_type_id"), "50");
    assert_eq!(v("RISK", "rsrc_id"), "250");
    assert_eq!(v("RISK", "risk_type_id"), "50");
    assert_eq!(v("PROJISSU", "rsrc_id"), "250");
    assert_eq!(v("PROJISSU", "thresh_parm_id"), "80");
}

/// Every standard column that names a workspace table's key is a known
/// reference, so a renumbered workspace row never leaves a stale pointer.
#[test]
fn every_standard_column_naming_a_workspace_key_is_a_reference() {
    let mut tables: BTreeSet<&str> = crate::TABLE_ORDER.iter().copied().collect();
    tables.extend(records::DETAIL_TABLES);
    tables.extend(crate::enterprise::TABLES);
    tables.insert("RISK");
    let mut missing = vec![];
    for table in tables {
        let fields: &[&str] = match records::standard_fields(table) {
            [] => crate::standard_fields(table).unwrap_or(&[]),
            f => f,
        };
        for field in fields {
            for (target, key) in WORKSPACE_TABLES {
                if *key != Some(*field) || target.eq_ignore_ascii_case(table) {
                    continue;
                }
                let known = REFERENCES
                    .iter()
                    .any(|(s, c, t)| s.eq_ignore_ascii_case(table) && c == field && t == target);
                if !known {
                    missing.push(format!("{table}.{field} -> {target}"));
                }
            }
        }
    }
    assert!(missing.is_empty(), "not in REFERENCES: {missing:?}");
}

#[test]
fn referenced_ids_lists_what_project_rows_use() {
    let doc = synthetic(1, "ALPHA");
    let used = referenced_ids(&doc);
    for key in [
        ("CALENDAR", 2),
        ("RSRC", 7),
        ("ACTVCODE", 41),
        ("ACTVTYPE", 40),
        ("UDFTYPE", 7),
        ("OBS", 5),
    ] {
        assert!(used.contains(&(key.0.to_string(), key.1)), "{key:?}");
    }
    assert!(!used.contains(&("CALENDAR".to_string(), 1)));
}

#[test]
fn used_subset_drops_workspace_rows_the_project_does_not_use() {
    let doc = synthetic(1, "ALPHA");
    let (_, mut workspace) = split(&doc);
    workspace.table_mut("CALENDAR").rows.push(
        "9|N|Unused|||CA_Base|x"
            .split('|')
            .map(String::from)
            .collect(),
    );
    let mut project = split(&doc).0;
    hydrate(&mut project, &workspace);
    let used = used_subset(&project, 1);
    let ids: Vec<String> = used
        .table("CALENDAR")
        .unwrap()
        .rows
        .iter()
        .map(|r| r[0].clone())
        .collect();
    // 2 is the project's calendar and the base of 3, its own; 4 is the
    // operator's calendar; nothing uses 1 or 9.
    assert_eq!(ids, ["2", "3", "4"]);
    assert_eq!(rows_of(&used, "RSRC"), 1, "nothing uses the helper");
    assert_eq!(rows_of(&used, "RSRCRATE"), 1, "rates follow their resource");
    assert_eq!(rows_of(&used, "CURRTYPE"), 1, "small tables are kept whole");
}

#[test]
fn next_id_stays_above_the_id_floor() {
    let mut doc = synthetic(1, "ALPHA");
    assert_eq!(records::next_id(&doc, "CALENDAR", "clndr_id"), 5);
    doc.id_floor.insert("CALENDAR".into(), 40);
    assert_eq!(records::next_id(&doc, "calendar", "clndr_id"), 41);
    assert_eq!(records::next_id(&doc, "RSRC", "rsrc_id"), 9);
}

#[test]
fn table_highs_count_rows_of_every_scope() {
    let highs = table_highs(&synthetic(1, "ALPHA"));
    assert_eq!(highs["CALENDAR"], 4);
    assert_eq!(highs["ACTVTYPE"], 45);
    assert_eq!(highs["ACTVCODE"], 46);
    assert!(!highs.contains_key("TASK"));
}

#[test]
fn every_corpus_file_splits_and_hydrates_back_exactly() {
    let Some(files) = corpus() else { return };
    for path in files {
        let doc = Document::read(&path).unwrap();
        let (project, workspace) = split(&doc);
        assert!(
            !workspace.tables.is_empty(),
            "{}: no workspace rows",
            path.display()
        );
        let mut stored = Document::parse(&project.text()).unwrap();
        let dict = Document::parse(&workspace.text()).unwrap();
        let report = hydrate(&mut stored, &dict);
        assert!(report.shadowed.is_empty(), "{}", path.display());
        stored.windows_encoding = doc.windows_encoding;
        assert!(
            stored.text() == doc.text(),
            "{}: text differs after split and hydrate",
            path.display()
        );
        assert_eq!(stored.to_bytes(), doc.to_bytes(), "{}", path.display());
    }
}

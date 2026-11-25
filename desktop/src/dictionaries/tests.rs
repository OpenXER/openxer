//! Dictionary edits: add, rename and delete-in-use per dictionary, the tree
//! moves and clipboard, their effect on the Activity Details tabs and grid
//! columns, export and reimport, undo and redo through the dialogs, and
//! software renders of every dialog (PNGs in the ignored `artifacts/`).
use super::*;
use crate::details::DetailTab;
use crate::grid::columns;
use crate::headless_render::SoftwarePainter;
use crate::store::Store;
use ops::*;

const ACT: CodeKind = CodeKind::Activity;

fn task(p: &ProjectData, i: usize) -> i64 {
    p.model.activities[i].task_id
}

fn reread(p: &ProjectData) -> ProjectData {
    let text = p.updated_document().text();
    ProjectData::from_document(
        Document::parse(&text).unwrap(),
        p.model.project.proj_id,
        "re".into(),
    )
    .unwrap()
}

fn shorts(doc: &Document, kind: CodeKind, type_id: i64) -> Vec<(String, usize)> {
    code_values(doc, kind, type_id)
        .into_iter()
        .map(|v| (v.short, v.depth))
        .collect()
}

fn rows(doc: &Document, table: &str) -> usize {
    doc.table(table).map_or(0, |t| t.rows.len())
}

/// Area (project) with N, S and P under S; P is assigned to activity 0, N to 1.
fn seeded_codes(p: &mut ProjectData) -> (i64, [i64; 3]) {
    let pid = p.model.project.proj_id;
    let (t0, t1) = (task(p, 0), task(p, 1));
    let d = &mut p.document;
    let area = add_code_type(d, ACT, Scope::Project, pid, Some("Area")).unwrap();
    let n = add_code_value(d, ACT, area, None).unwrap();
    set_code_value(d, ACT, area, n, ValueField::Short, "N").unwrap();
    set_code_value(d, ACT, area, n, ValueField::Name, "North yard").unwrap();
    let s = add_code_value(d, ACT, area, Some(n)).unwrap();
    set_code_value(d, ACT, area, s, ValueField::Short, "S").unwrap();
    let pumps = add_code_value(d, ACT, area, Some(s)).unwrap();
    set_code_value(d, ACT, area, pumps, ValueField::Short, "P").unwrap();
    let spec = value_spec(d, ACT, area);
    assert!(indent(d, &spec, Some(area), pumps).unwrap());
    for (t, v) in [(t0, pumps), (t1, n)] {
        records::add_row(
            d,
            "TASKACTV",
            &[
                ("task_id", t.to_string()),
                ("actv_code_type_id", area.to_string()),
                ("actv_code_id", v.to_string()),
                ("proj_id", pid.to_string()),
            ],
        );
    }
    (area, [n, s, pumps])
}

#[test]
fn activity_codes_add_rename_shift_and_delete_in_use() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let (area, [n, s, pumps]) = seeded_codes(&mut p);
    let d = &mut p.document;
    assert_eq!(
        shorts(d, ACT, area),
        [("N".into(), 0), ("S".into(), 0), ("P".into(), 1)]
    );
    // Names are unique per dictionary, values per level, both ignoring case.
    assert!(add_code_type(d, ACT, Scope::Global, pid, Some("area")).is_err());
    assert!(set_code_value(d, ACT, area, s, ValueField::Short, "n").is_err());
    assert!(set_code_value(d, ACT, area, s, ValueField::Short, " ").is_err());
    assert!(set_code_value(d, ACT, area, n, ValueField::Color, "red").is_err());
    set_code_value(d, ACT, area, n, ValueField::Color, "ff8000").unwrap();
    assert_eq!(code_values(d, ACT, area)[0].color, "FF8000");
    // New values get the defaults the export needs.
    assert_eq!(
        value(d, &value_spec(d, ACT, area), pumps, "total_assignments"),
        "0"
    );
    // Shift S (with its child) above N, then P back out to the top level.
    let spec = value_spec(d, ACT, area);
    assert!(shift(d, &spec, Some(area), s, -1));
    assert_eq!(
        shorts(d, ACT, area),
        [("S".into(), 0), ("P".into(), 1), ("N".into(), 0)]
    );
    assert!(!shift(d, &spec, Some(area), s, -1), "already first");
    assert!(outdent(d, &spec, Some(area), pumps).unwrap());
    assert_eq!(
        shorts(d, ACT, area),
        [("S".into(), 0), ("P".into(), 0), ("N".into(), 0)]
    );
    assert!(indent(d, &spec, Some(area), pumps).unwrap());
    // Max Length: 1..=20 and never below the longest value.
    assert!(set_code_length(d, ACT, area, 0).is_err());
    set_code_length(d, ACT, area, 1).unwrap();
    assert!(set_code_value(d, ACT, area, n, ValueField::Short, "NN").is_err());
    set_code_length(d, ACT, area, 20).unwrap();
    set_code_value(d, ACT, area, n, ValueField::Short, "NN").unwrap();
    assert!(set_code_length(d, ACT, area, 1).is_err());
    // Deleting S removes P with it and P's assignment, and keeps N's.
    assert_eq!(code_value_usage(d, ACT, area, s), 1);
    assert_eq!(delete_code_value(d, ACT, area, s), 1);
    assert_eq!(shorts(d, ACT, area), [("NN".into(), 0)]);
    assert_eq!(rows(d, "TASKACTV"), 1);
    // Renaming and deleting the code type.
    rename_code_type(d, ACT, area, "Zone").unwrap();
    assert_eq!(code_types(d, ACT, Scope::Project, pid)[0].name, "Zone");
    assert!(rename_code_type(d, ACT, area, "").is_err());
    assert_eq!(code_type_usage(d, ACT, area), (1, 1));
    assert_eq!(delete_code_type(d, ACT, area), 1);
    assert_eq!(rows(d, "TASKACTV"), 0);
    assert_eq!(rows(d, "ACTVCODE"), 0);
    assert!(code_types(d, ACT, Scope::Project, pid).is_empty());
}

#[test]
fn activity_code_scopes_widen_only() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let d = &mut p.document;
    let id = add_code_type(d, ACT, Scope::Project, pid, None).unwrap();
    let other = add_code_type(d, ACT, Scope::Project, pid, None).unwrap();
    assert_eq!(code_type(d, ACT, other).unwrap().name, "New Code 2");
    let spec = TreeSpec {
        table: "ACTVTYPE",
        id: "actv_code_type_id",
        parent: None,
        seq: "seq_num",
        key: "actv_code_type",
        owner: None,
        key_len: None,
    };
    assert_eq!(value(d, &spec, id, "proj_id"), pid.to_string());
    assert_eq!(value(d, &spec, id, "actv_code_type_scope"), "AS_Project");
    set_code_scope(d, ACT, id, Scope::Eps, pid).unwrap();
    assert_eq!(value(d, &spec, id, "actv_code_type_scope"), "AS_EPS");
    assert_eq!(value(d, &spec, id, "proj_id"), "");
    assert!(set_code_scope(d, ACT, id, Scope::Eps, pid).is_err());
    set_code_scope(d, ACT, id, Scope::Global, pid).unwrap();
    assert_eq!(code_types(d, ACT, Scope::Global, pid)[0].id, id);
    assert_eq!(code_types(d, ACT, Scope::Project, pid)[0].id, other);
    // Shift up/down keeps the other scopes where they are.
    let third = add_code_type(d, ACT, Scope::Project, pid, Some("Third")).unwrap();
    assert!(shift_code_type(d, ACT, Scope::Project, pid, third, -1));
    let order: Vec<i64> = code_types(d, ACT, Scope::Project, pid)
        .iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(order, [third, other]);
}

#[test]
fn code_clipboard_moves_cut_values_and_copies_with_new_keys() {
    let mut p = openxer_xer::starter();
    let (area, [n, s, pumps]) = seeded_codes(&mut p);
    let d = &mut p.document;
    let spec = value_spec(d, ACT, area);
    // Copy S (with P) after N: new rows, keys made unique, no assignments.
    let copy = clip(d, &spec, Some(area), s, false);
    let added = paste(d, &spec, Some(area), Some(n), &copy).unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(
        shorts(d, ACT, area),
        [
            ("N".into(), 0),
            ("S2".into(), 0),
            ("P".into(), 1),
            ("S".into(), 0),
            ("P".into(), 1)
        ]
    );
    assert_eq!(rows(d, "TASKACTV"), 2);
    // Cut P and paste it after N: same id, assignment kept, now top level.
    let cut = clip(d, &spec, Some(area), pumps, true);
    let moved = paste(d, &spec, Some(area), Some(n), &cut).unwrap();
    assert_eq!(moved, [pumps]);
    let v = code_values(d, ACT, area);
    assert_eq!(v[1].id, pumps);
    assert_eq!(v[1].depth, 0);
    assert_eq!(code_value_usage(d, ACT, area, pumps), 1);
    // A value cannot be pasted under itself, and cut values stay in their code.
    let cut = clip(d, &spec, Some(area), s, true);
    let child = code_values(d, ACT, area)
        .into_iter()
        .find(|v| v.parent == Some(added[0]))
        .unwrap();
    assert!(paste(d, &spec, Some(area), Some(s), &cut).is_err());
    assert!(paste(d, &spec, Some(area + 1), None, &cut).is_err());
    let _ = child;
}

#[test]
fn project_and_resource_codes_delete_their_assignments() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let d = &mut p.document;
    let kind = CodeKind::Project;
    let region = add_code_type(d, kind, Scope::Global, pid, Some("Region")).unwrap();
    let gulf = add_code_value(d, kind, region, None).unwrap();
    set_code_value(d, kind, region, gulf, ValueField::Short, "GULF").unwrap();
    set_code_value(d, kind, region, gulf, ValueField::Name, "Gulf Coast").unwrap();
    records::add_row(
        d,
        "PROJPCAT",
        &[
            ("proj_id", pid.to_string()),
            ("proj_catg_type_id", region.to_string()),
            ("proj_catg_id", gulf.to_string()),
        ],
    );
    assert_eq!(code_value_usage(d, kind, region, gulf), 1);
    assert_eq!(delete_code_value(d, kind, region, gulf), 1);
    assert_eq!(rows(d, "PROJPCAT"), 0);
    let kind = CodeKind::Resource;
    let craft = add_code_type(d, kind, Scope::Global, pid, Some("Craft")).unwrap();
    let pipe = add_code_value(d, kind, craft, None).unwrap();
    set_code_value(d, kind, craft, pipe, ValueField::Short, "PF").unwrap();
    records::add_row(
        d,
        "RSRCRCAT",
        &[
            ("rsrc_id", "7".into()),
            ("rsrc_catg_type_id", craft.to_string()),
            ("rsrc_catg_id", pipe.to_string()),
        ],
    );
    rename_code_type(d, kind, craft, "Trade").unwrap();
    assert_eq!(code_types(d, kind, Scope::Global, pid)[0].name, "Trade");
    assert_eq!(code_type_usage(d, kind, craft), (1, 1));
    assert_eq!(delete_code_type(d, kind, craft), 1);
    assert_eq!(rows(d, "RSRCRCAT"), 0);
    assert_eq!(rows(d, "RCATVAL"), 0);
    assert_eq!(rows(d, "PCATTYPE"), 1);
}

#[test]
fn user_defined_fields_add_rename_type_and_delete_in_use() {
    let mut p = openxer_xer::starter();
    let (pid, t0) = (p.model.project.proj_id, task(&p, 0));
    let d = &mut p.document;
    let a = add_udf(d, "TASK");
    let b = add_udf(d, "TASK");
    let r = add_udf(d, "RSRC");
    assert_eq!(
        udfs(d, "TASK")
            .iter()
            .map(|u| u.title.clone())
            .collect::<Vec<_>>(),
        ["(New)", "(New) 2"]
    );
    assert_eq!(
        udfs(d, "RSRC")[0].title,
        "(New)",
        "titles are per subject area"
    );
    set_udf_title(d, a, "Crew").unwrap();
    assert!(set_udf_title(d, b, "crew").is_err());
    assert!(set_udf_title(d, r, "Crew").is_ok());
    set_udf_type(d, a, "FT_FLOAT_2_DECIMALS").unwrap();
    assert!(set_udf_type(d, a, "FT_BOGUS").is_err());
    assert_eq!(data_type_label("FT_FLOAT_2_DECIMALS"), "Number");
    assert_eq!(
        value(
            d,
            &TreeSpec {
                table: "UDFTYPE",
                id: "udf_type_id",
                parent: None,
                seq: "udf_type_id",
                key: "udf_type_label",
                owner: None,
                key_len: None
            },
            a,
            "udf_type_name"
        ),
        format!("user_field_{a}")
    );
    records::add_row(
        d,
        "UDFVALUE",
        &[
            ("udf_type_id", a.to_string()),
            ("fk_id", t0.to_string()),
            ("proj_id", pid.to_string()),
            ("udf_number", "12".into()),
        ],
    );
    assert_eq!(
        udfs(d, "TASK").iter().find(|u| u.id == a).unwrap().values,
        1
    );
    assert!(
        set_udf_type(d, a, "FT_TEXT").is_err(),
        "the field holds values"
    );
    assert_eq!(delete_udf(d, a), 1);
    assert_eq!(rows(d, "UDFVALUE"), 0);
    assert_eq!(udfs(d, "TASK").len(), 1);
}

#[test]
fn cost_accounts_delete_moves_or_clears_their_users() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let d = &mut p.document;
    let site = add_account(d, None).unwrap();
    set_account(d, site, AccountField::Id, "100").unwrap();
    set_account(d, site, AccountField::Name, "Site work").unwrap();
    set_account(d, site, AccountField::Description, "Earthwork & <grading>").unwrap();
    assert_eq!(account_description(d, site), "Earthwork & <grading>");
    let child = add_account(d, Some(site)).unwrap();
    set_account(d, child, AccountField::Id, "110").unwrap();
    assert!(indent(d, &ACCOUNT, None, child).unwrap());
    let other = add_account(d, Some(site)).unwrap();
    set_account(d, other, AccountField::Id, "200").unwrap();
    assert!(set_account(d, other, AccountField::Name, " ").is_err());
    assert_eq!(
        accounts(d)
            .iter()
            .map(|a| (a.short.as_str(), a.depth))
            .collect::<Vec<_>>(),
        [("100", 0), ("110", 1), ("200", 0)]
    );
    records::add_row(
        d,
        "PROJCOST",
        &[("cost_item_id", "1".into()), ("acct_id", child.to_string())],
    );
    records::add_row(
        d,
        "TASKRSRC",
        &[("taskrsrc_id", "1".into()), ("acct_id", site.to_string())],
    );
    let project_row = d
        .table("PROJECT")
        .unwrap()
        .rows
        .iter()
        .position(|_| true)
        .unwrap();
    records::set(d, "PROJECT", project_row, "acct_id", site.to_string());
    assert_eq!(account_usage(d, site), 3);
    assert!(
        delete_account(d, site, Some(child)).is_err(),
        "child is deleted too"
    );
    assert_eq!(delete_account(d, site, Some(other)).unwrap(), 3);
    assert_eq!(records::get(d, "PROJCOST", 0, "acct_id"), other.to_string());
    assert_eq!(records::get(d, "TASKRSRC", 0, "acct_id"), other.to_string());
    assert_eq!(
        records::get(d, "PROJECT", project_row, "acct_id"),
        other.to_string()
    );
    assert_eq!(accounts(d).len(), 1);
    assert_eq!(delete_account(d, other, None).unwrap(), 3);
    assert_eq!(records::get(d, "PROJCOST", 0, "acct_id"), "");
    assert_eq!(rows(d, "ACCOUNT"), 0);
    let _ = pid;
}

#[test]
fn admin_categories_add_rename_and_delete_in_use() {
    let mut p = openxer_xer::starter();
    let d = &mut p.document;
    // (category, referencing table, column, rows kept after delete)
    for (category, table, column, kept) in [
        (Category::Expense, "PROJCOST", "cost_type_id", true),
        (Category::Documents, "DOCUMENT", "doc_catg_id", true),
        (Category::DocStatus, "DOCUMENT", "doc_status_id", true),
        (Category::Risk, "PROJRISK", "risk_type_id", true),
        (Category::Units, "RSRC", "unit_id", true),
        (Category::Notebook, "TASKMEMO", "memo_type_id", false),
    ] {
        let spec = category.spec();
        let first = add_category(d, category, None).unwrap().unwrap();
        let second = add_category(d, category, Some(first)).unwrap().unwrap();
        let names: Vec<String> = categories(d, category)
            .iter()
            .map(|r| r.cells[0].clone())
            .collect();
        assert_eq!(names.len(), 2, "{category:?}");
        assert_ne!(names[0], names[1]);
        set_category(d, category, second, spec.tree.key, "Second").unwrap();
        assert!(
            set_category(d, category, first, spec.tree.key, "second").is_err(),
            "{category:?} names are unique"
        );
        assert!(shift(d, &spec.tree, None, second, -1));
        assert_eq!(categories(d, category)[0].id, second);
        let before = rows(d, table);
        records::add_row(
            d,
            table,
            &[(column, second.to_string()), ("proj_id", "1".into())],
        );
        assert_eq!(category_usage(d, category, second), 1);
        assert_eq!(delete_category(d, category, second), 1);
        assert_eq!(categories(d, category).len(), 1);
        if kept {
            assert_eq!(rows(d, table), before + 1);
            let t = d.table(table).unwrap();
            assert_eq!(t.value(t.rows.last().unwrap(), column), "", "{category:?}");
        } else {
            assert_eq!(
                rows(d, table),
                before,
                "notebooks under the topic are removed"
            );
        }
    }
    // Notebook topics carry the four flags; Units carry a name too.
    let topic = categories(d, Category::Notebook)[0].clone();
    assert_eq!(topic.cells[1..], ["Y", "Y", "Y", "Y"]);
    set_category(d, Category::Notebook, topic.id, "task_flag", "N").unwrap();
    assert_eq!(categories(d, Category::Notebook)[0].cells[4], "N");
    assert_eq!(categories(d, Category::Units)[0].cells.len(), 2);
    // Risk categories nest.
    let parent = categories(d, Category::Risk)[0].id;
    let child = add_category(d, Category::Risk, Some(parent))
        .unwrap()
        .unwrap();
    let risk = category_tree(Category::Risk);
    assert!(indent(d, &risk, None, child).unwrap());
    assert_eq!(categories(d, Category::Risk)[1].depth, 1);
    assert_eq!(delete_category(d, Category::Risk, parent), 0);
    assert!(
        categories(d, Category::Risk).is_empty(),
        "children go with the parent"
    );
    // Baseline types: a flat list in BASETYPE ordered by its own sequence.
    assert!(categories(d, Category::Baseline).is_empty());
    let first = add_category(d, Category::Baseline, None).unwrap().unwrap();
    let second = add_category(d, Category::Baseline, Some(first))
        .unwrap()
        .unwrap();
    set_category(d, Category::Baseline, first, "base_type", "Initial Plan").unwrap();
    assert!(set_category(d, Category::Baseline, second, "base_type", "initial plan").is_err());
    set_category(d, Category::Baseline, second, "base_type", "Revised Plan").unwrap();
    let names: Vec<_> = categories(d, Category::Baseline)
        .into_iter()
        .map(|r| r.cells[0].clone())
        .collect();
    assert_eq!(names, ["Initial Plan", "Revised Plan"]);
    assert_eq!(
        d.table("BASETYPE").unwrap().fields,
        ["base_type_id", "base_type_seq_num", "base_type"]
    );
    assert_eq!(delete_category(d, Category::Baseline, first), 0);
    assert_eq!(categories(d, Category::Baseline).len(), 1);
}

/// Every table a dictionary edit can change is in the undo snapshot.
#[test]
fn undo_snapshots_cover_every_dictionary_table() {
    let mut tables: Vec<&str> = vec![
        "UDFTYPE", "UDFVALUE", "UDFCODE", "ACCOUNT", "PROJCOST", "TASKRSRC", "PROJECT",
    ];
    for kind in [CodeKind::Activity, CodeKind::Project, CodeKind::Resource] {
        let t = kind.tables();
        tables.extend([t.types, t.value.table, t.assign]);
    }
    for c in Category::ALL {
        let spec = c.spec();
        tables.push(spec.tree.table);
        tables.extend(spec.refs.iter().map(|r| r.0));
    }
    for t in tables {
        assert!(
            records::DETAIL_TABLES.contains(&t),
            "{t} is not snapshotted"
        );
    }
}

#[test]
fn dictionaries_survive_export_and_reimport_in_xer_table_order() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let (area, _) = seeded_codes(&mut p);
    let d = &mut p.document;
    let region = add_code_type(d, CodeKind::Project, Scope::Global, pid, Some("Region")).unwrap();
    add_code_value(d, CodeKind::Project, region, None).unwrap();
    let craft = add_code_type(d, CodeKind::Resource, Scope::Global, pid, Some("Craft")).unwrap();
    add_code_value(d, CodeKind::Resource, craft, None).unwrap();
    let crew = add_udf(d, "TASK");
    set_udf_title(d, crew, "Crew").unwrap();
    let site = add_account(d, None).unwrap();
    set_account(d, site, AccountField::Description, "Line one\nLine two").unwrap();
    for c in Category::ALL {
        add_category(d, c, None).unwrap();
    }
    let again = reread(&p);
    let d2 = &again.document;
    assert_eq!(shorts(d2, ACT, area), shorts(&p.document, ACT, area));
    assert_eq!(
        code_types(d2, CodeKind::Project, Scope::Global, pid)[0].name,
        "Region"
    );
    assert_eq!(
        code_types(d2, CodeKind::Resource, Scope::Global, pid)[0].name,
        "Craft"
    );
    assert_eq!(udfs(d2, "TASK")[0].title, "Crew");
    assert_eq!(account_description(d2, site), "Line one\nLine two");
    for c in Category::ALL {
        assert_eq!(categories(d2, c), categories(&p.document, c), "{c:?}");
    }
    // Tables sit in the reference export order, and the ones created here carry its columns.
    let rank = |n: &str| openxer_xer::TABLE_ORDER.iter().position(|t| *t == n);
    let order: Vec<_> = d2.tables.iter().filter_map(|t| rank(&t.name)).collect();
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "{:?}",
        d2.tables.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
    for name in [
        "ACTVTYPE", "ACTVCODE", "PCATTYPE", "PCATVAL", "RCATTYPE", "RCATVAL", "UDFTYPE", "ACCOUNT",
        "COSTTYPE", "DOCCATG", "DOCSTAT", "RISKTYPE", "MEMOTYPE", "UMEASURE",
    ] {
        let t = d2.table(name).unwrap_or_else(|| panic!("{name} exported"));
        assert_eq!(
            t.fields,
            openxer_xer::standard_fields(name).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn grid_columns_and_index_follow_the_dictionaries() {
    let mut p = openxer_xer::starter();
    let (area, [_, _, pumps]) = seeded_codes(&mut p);
    let crew = add_udf(&mut p.document, "TASK");
    set_udf_title(&mut p.document, crew, "Crew").unwrap();
    set_udf_type(&mut p.document, crew, "FT_MONEY").unwrap();
    let has = |p: &ProjectData, id: &str| columns::catalog(p).iter().any(|c| c.id == id);
    assert!(has(&p, "code:Area"));
    let udf = columns::catalog(&p)
        .into_iter()
        .find(|c| c.id == "udf:Crew")
        .unwrap();
    assert_eq!(udf.kind, columns::Kind::Cost);
    let ix = columns::Index::new((false, 0.));
    assert_eq!(ix.code_of(&p, task(&p, 0), area).unwrap().short_name, "P");
    rename_code_type(&mut p.document, ACT, area, "Zone").unwrap();
    set_code_value(&mut p.document, ACT, area, pumps, ValueField::Short, "PMP").unwrap();
    assert!(has(&p, "code:Zone") && !has(&p, "code:Area"));
    let ix = columns::Index::new((false, 0.));
    assert_eq!(ix.code_of(&p, task(&p, 0), area).unwrap().short_name, "PMP");
    delete_code_type(&mut p.document, ACT, area);
    delete_udf(&mut p.document, crew);
    assert!(!has(&p, "code:Zone") && !has(&p, "udf:Crew"));
}

// ------------------------------------------------------------ through the UI

fn temp_db() -> std::path::PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openxer-dictionaries-{}-{unique}.db",
        std::process::id()
    ))
}

fn open_app(
    project: ProjectData,
) -> (egui::Context, App, SoftwarePainter, f64, std::path::PathBuf) {
    let path = temp_db();
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut project = project;
    schedule_model(&mut project.model, &project.resources).unwrap();
    let mut app = App::new(&ctx, store, project);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    (ctx, app, painter, time, path)
}

fn cleanup(path: std::path::PathBuf) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

fn idle(app: &mut App, ctx: &egui::Context, painter: &mut SoftwarePainter, time: &mut f64) {
    for _ in 0..2 {
        let _ = painter.frame(app, ctx, time, vec![], false);
    }
}

/// Open a main menu by clicking its title.
fn open_menu(
    app: &mut App,
    ctx: &egui::Context,
    painter: &mut SoftwarePainter,
    time: &mut f64,
    title: &str,
) {
    *time += 0.1;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    let pos = output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == title => {
                Some(t.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("No menu {title}"));
    for pressed in [true, false] {
        let _ = painter.frame(
            app,
            ctx,
            time,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            false,
        );
    }
    idle(app, ctx, painter, time);
}

fn key(key: Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }
}

/// Replace the text of the focused editor and press Enter.
fn type_text(
    app: &mut App,
    ctx: &egui::Context,
    painter: &mut SoftwarePainter,
    time: &mut f64,
    text: &str,
) {
    let mut events: Vec<egui::Event> = (0..30).map(|_| key(Key::Backspace)).collect();
    events.push(egui::Event::Text(text.into()));
    let _ = painter.frame(app, ctx, time, events, false);
    let _ = painter.frame(app, ctx, time, vec![key(Key::Enter)], false);
    idle(app, ctx, painter, time);
}

fn texts(app: &mut App, ctx: &egui::Context, time: &mut f64) -> Vec<String> {
    *time += 0.1;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
    output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn activity_codes_dialog_edits_are_single_undo_steps() {
    let (ctx, mut app, mut painter, mut time, path) = open_app(openxer_xer::starter());
    let pid = app.project.model.project.proj_id;
    // The toolbar button opens the dialog; with no project codes it says so first.
    painter.click_action(&mut app, &ctx, &mut time, "tool_activity_codes");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert!(app.dictionaries.codes.is_some());
    assert!(app.dictionaries.notice.is_some());
    painter.click_action(&mut app, &ctx, &mut time, "dictionary_notice_ok");
    assert!(app.dictionaries.notice.is_none());
    // Modify > Add creates a project code type: one undo step.
    painter.click_action(&mut app, &ctx, &mut time, "codes_modify");
    idle(&mut app, &ctx, &mut painter, &mut time);
    let (undo0, _) = app.history.counts();
    painter.click_action(&mut app, &ctx, &mut time, "code_types_add");
    idle(&mut app, &ctx, &mut painter, &mut time);
    let types = code_types(&app.project.document, ACT, Scope::Project, pid);
    assert_eq!(types.len(), 1);
    assert_eq!(app.history.counts().0, undo0 + 1);
    // Rename it in place: click the selected name cell, type, Enter.
    painter.click_action(&mut app, &ctx, &mut time, "code_types_cell_0_0");
    idle(&mut app, &ctx, &mut painter, &mut time);
    type_text(&mut app, &ctx, &mut painter, &mut time, "Area");
    assert_eq!(
        code_types(&app.project.document, ACT, Scope::Project, pid)[0].name,
        "Area"
    );
    assert_eq!(app.history.counts().0, undo0 + 2);
    painter.click_action(&mut app, &ctx, &mut time, "code_types_close");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert_eq!(
        app.dictionaries.codes.as_ref().unwrap().type_id,
        Some(types[0].id)
    );
    // Add a value; the editor opens on its code value.
    painter.click_action(&mut app, &ctx, &mut time, "codes_add");
    idle(&mut app, &ctx, &mut painter, &mut time);
    type_text(&mut app, &ctx, &mut painter, &mut time, "N");
    let area = types[0].id;
    let values = code_values(&app.project.document, ACT, area);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].short, "N");
    assert_eq!(app.history.counts().0, undo0 + 4);
    // The Details Codes tab and the grid show the assignment.
    app.assign_code(0, area, values[0].id);
    app.selected = Some(0);
    app.detail.tab = DetailTab::Codes;
    let shown = texts(&mut app, &ctx, &mut time);
    assert!(shown.iter().any(|t| t == "Area"), "{shown:?}");
    let undo_assigned = app.history.counts().0;
    // Delete the value in use: asked first, then one undo step removes both.
    painter.click_action(&mut app, &ctx, &mut time, "codes_delete");
    idle(&mut app, &ctx, &mut painter, &mut time);
    let confirm = app.dictionaries.confirm.as_ref().expect("asks first");
    assert!(
        confirm.text.contains("assigned to 1 activity"),
        "{}",
        confirm.text
    );
    painter.click_action(&mut app, &ctx, &mut time, "dictionary_confirm_yes");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert!(code_values(&app.project.document, ACT, area).is_empty());
    assert_eq!(rows(&app.project.document, "TASKACTV"), 0);
    assert_eq!(app.history.counts().0, undo_assigned + 1);
    app.history_move(&ctx, false);
    assert_eq!(code_values(&app.project.document, ACT, area).len(), 1);
    assert_eq!(rows(&app.project.document, "TASKACTV"), 1);
    app.history_move(&ctx, true);
    assert_eq!(rows(&app.project.document, "TASKACTV"), 0);
    // Undo back to before the type existed.
    while app.history.counts().0 > undo0 {
        app.history_move(&ctx, false);
    }
    assert!(code_types(&app.project.document, ACT, Scope::Project, pid).is_empty());
    painter.click_action(&mut app, &ctx, &mut time, "codes_close");
    assert!(!app.dictionary_open());
    // The edits survive a save and reopen of the workspace.
    app.history_move(&ctx, true);
    app.history_move(&ctx, true);
    app.save();
    let stored = Store::open(path.clone()).unwrap().last().unwrap().unwrap();
    assert_eq!(
        code_types(&stored.document, ACT, Scope::Project, pid)[0].name,
        "Area"
    );
    drop(app);
    cleanup(path);
}

#[test]
fn udf_dialog_feeds_the_details_tab_and_renames_layout_columns() {
    let (ctx, mut app, mut painter, mut time, path) = open_app(openxer_xer::starter());
    painter.click_action(&mut app, &ctx, &mut time, "tool_udf");
    idle(&mut app, &ctx, &mut painter, &mut time);
    // The dialog is modal: Delete and Insert do not reach the activity table.
    let count = app.project.model.activities.len();
    let _ = painter.frame(
        &mut app,
        &ctx,
        &mut time,
        vec![key(Key::Delete), key(Key::Insert)],
        false,
    );
    assert!(app.delete_confirm.is_none());
    assert_eq!(app.project.model.activities.len(), count);
    painter.click_action(&mut app, &ctx, &mut time, "udf_add");
    idle(&mut app, &ctx, &mut painter, &mut time);
    type_text(&mut app, &ctx, &mut painter, &mut time, "Crew");
    let id = udfs(&app.project.document, "TASK")[0].id;
    assert_eq!(udfs(&app.project.document, "TASK")[0].title, "Crew");
    // Show it as a grid column, then rename: the layout keeps the column.
    app.grid
        .layout
        .columns
        .push(crate::grid::layout::ColumnSetting::new("udf:Crew", 80.));
    app.run_dictionary(Cmd::UdfTitle(id, "Crew Size".into()))
        .unwrap();
    assert!(app
        .grid
        .layout
        .columns
        .iter()
        .any(|c| c.id == "udf:Crew Size"));
    app.selected = Some(0);
    app.detail.tab = DetailTab::UserDefinedFields;
    let shown = texts(&mut app, &ctx, &mut time);
    assert!(shown.iter().any(|t| t == "Crew Size"), "{shown:?}");
    // Delete through the dialog: no values, no question.
    painter.click_action(&mut app, &ctx, &mut time, "udf_delete");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert!(udfs(&app.project.document, "TASK").is_empty());
    app.history_move(&ctx, false);
    assert_eq!(udfs(&app.project.document, "TASK")[0].title, "Crew Size");
    drop(app);
    cleanup(path);
}

#[test]
fn cost_account_in_use_asks_for_a_replacement() {
    let mut p = openxer_xer::starter();
    let a = add_account(&mut p.document, None).unwrap();
    let b = add_account(&mut p.document, Some(a)).unwrap();
    records::add_row(
        &mut p.document,
        "PROJCOST",
        &[("cost_item_id", "1".into()), ("acct_id", a.to_string())],
    );
    let (ctx, mut app, mut painter, mut time, path) = open_app(p);
    painter.click_action(&mut app, &ctx, &mut time, "tool_cost_accounts");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert_eq!(
        app.dictionaries.accounts.as_ref().unwrap().selected,
        Some(a)
    );
    painter.click_action(&mut app, &ctx, &mut time, "accounts_delete");
    idle(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "dictionary_confirm_yes");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert!(app
        .dictionaries
        .accounts
        .as_ref()
        .unwrap()
        .replace
        .is_some());
    app.dictionaries.accounts.as_mut().unwrap().replace = Some((a, Some(b)));
    idle(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "accounts_replacement_select");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert_eq!(accounts(&app.project.document).len(), 1);
    assert_eq!(
        records::get(&app.project.document, "PROJCOST", 0, "acct_id"),
        b.to_string()
    );
    app.history_move(&ctx, false);
    assert_eq!(accounts(&app.project.document).len(), 2);
    assert_eq!(
        records::get(&app.project.document, "PROJCOST", 0, "acct_id"),
        a.to_string()
    );
    drop(app);
    cleanup(path);
}

#[test]
fn admin_categories_tabs_and_notebook_topic_delete() {
    let mut p = openxer_xer::starter();
    let topic = add_category(&mut p.document, Category::Notebook, None)
        .unwrap()
        .unwrap();
    let t0 = task(&p, 0);
    records::add_row(
        &mut p.document,
        "TASKMEMO",
        &[
            ("memo_id", "1".into()),
            ("task_id", t0.to_string()),
            ("memo_type_id", topic.to_string()),
            ("proj_id", p.model.project.proj_id.to_string()),
            ("task_memo", "<html><body>Hold point</body></html>".into()),
        ],
    );
    let (ctx, mut app, mut painter, mut time, path) = open_app(p);
    open_menu(&mut app, &ctx, &mut painter, &mut time, "Admin");
    painter.click_action(&mut app, &ctx, &mut time, "menu_admin_categories");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert_eq!(
        app.dictionaries.admin.as_ref().unwrap().tab,
        Category::Expense
    );
    // WBS Categories (XER carries no such list) was removed.
    assert!(!app.action_rects.contains_key("admin_tab_wbs"));
    painter.click_action(&mut app, &ctx, &mut time, "admin_add");
    idle(&mut app, &ctx, &mut painter, &mut time);
    type_text(&mut app, &ctx, &mut painter, &mut time, "Freight");
    assert_eq!(
        categories(&app.project.document, Category::Expense)[0].cells[0],
        "Freight"
    );
    painter.click_action(&mut app, &ctx, &mut time, "admin_tab_notebook");
    idle(&mut app, &ctx, &mut painter, &mut time);
    painter.click_action(&mut app, &ctx, &mut time, "admin_delete");
    idle(&mut app, &ctx, &mut painter, &mut time);
    let text = app.dictionaries.confirm.as_ref().unwrap().text.clone();
    assert!(text.contains("1 notebooks"), "{text}");
    painter.click_action(&mut app, &ctx, &mut time, "dictionary_confirm_yes");
    idle(&mut app, &ctx, &mut painter, &mut time);
    assert_eq!(rows(&app.project.document, "TASKMEMO"), 0);
    assert!(categories(&app.project.document, Category::Notebook).is_empty());
    app.history_move(&ctx, false);
    assert_eq!(rows(&app.project.document, "TASKMEMO"), 1);
    painter.click_action(&mut app, &ctx, &mut time, "admin_close");
    assert!(!app.dictionary_open());
    drop(app);
    cleanup(path);
}

/// A reference export with activity codes: deleting a value in use removes exactly its
/// assignments and every other row exports unchanged.
#[test]
fn corpus_code_value_delete_touches_only_its_rows() {
    let path = crate::test_corpus::file("5640.xer");
    let Ok(doc) = Document::read(&path) else {
        return;
    };
    let pid = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc, pid, "5640".into()).unwrap();
    let before = p.updated_document();
    let d = &mut p.document;
    let kind = ACT;
    let (type_id, value) = [Scope::Global, Scope::Eps, Scope::Project]
        .into_iter()
        .flat_map(|s| code_types(d, kind, s, pid))
        .find_map(|t| {
            code_values(d, kind, t.id)
                .into_iter()
                .find(|v| !v.has_children && code_value_usage(d, kind, t.id, v.id) > 0)
                .map(|v| (t.id, v.id))
        })
        .expect("a code value in use");
    let used = code_value_usage(d, kind, type_id, value);
    let assigned_before = rows(d, "TASKACTV");
    assert_eq!(delete_code_value(d, kind, type_id, value), used);
    assert_eq!(rows(d, "TASKACTV"), assigned_before - used);
    let after = Document::parse(&p.updated_document().text()).unwrap();
    for table in &before.tables {
        let new = after.table(&table.name).unwrap();
        if table.name == "TASKACTV" || table.name == "ACTVCODE" {
            let id = table.field("actv_code_id").unwrap();
            let kept: Vec<_> = table
                .rows
                .iter()
                .filter(|r| r[id] != value.to_string())
                .collect();
            assert_eq!(kept.len(), new.rows.len(), "{}", table.name);
            assert!(
                kept.iter().zip(&new.rows).all(|(a, b)| *a == b),
                "{}",
                table.name
            );
        } else {
            assert_eq!(table.rows, new.rows, "{}", table.name);
        }
    }
}

#[test]
fn render_dictionary_dialogs_to_png() {
    let artifacts = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let (area, _) = seeded_codes(&mut p);
    {
        let d = &mut p.document;
        for (short, name) in [("CIV", "Civil"), ("PIPE", "Piping"), ("ELEC", "Electrical")] {
            let v = add_code_value(d, ACT, area, None).unwrap();
            set_code_value(d, ACT, area, v, ValueField::Short, short).unwrap();
            set_code_value(d, ACT, area, v, ValueField::Name, name).unwrap();
        }
        add_code_type(d, ACT, Scope::Project, pid, Some("Phase")).unwrap();
        let region =
            add_code_type(d, CodeKind::Project, Scope::Global, pid, Some("Region")).unwrap();
        let v = add_code_value(d, CodeKind::Project, region, None).unwrap();
        set_code_value(
            d,
            CodeKind::Project,
            region,
            v,
            ValueField::Name,
            "Gulf Coast",
        )
        .unwrap();
        let craft =
            add_code_type(d, CodeKind::Resource, Scope::Global, pid, Some("Craft")).unwrap();
        let v = add_code_value(d, CodeKind::Resource, craft, None).unwrap();
        set_code_value(
            d,
            CodeKind::Resource,
            craft,
            v,
            ValueField::Name,
            "Pipefitter",
        )
        .unwrap();
        for (title, logical) in [
            ("Crew", "FT_TEXT"),
            ("Budget", "FT_MONEY"),
            ("Need Date", "FT_END_DATE"),
        ] {
            let u = add_udf(d, "TASK");
            set_udf_title(d, u, title).unwrap();
            set_udf_type(d, u, logical).unwrap();
        }
        let site = add_account(d, None).unwrap();
        set_account(d, site, AccountField::Id, "100").unwrap();
        set_account(d, site, AccountField::Name, "Site work").unwrap();
        set_account(d, site, AccountField::Description, "Earthwork and grading").unwrap();
        let fine = add_account(d, Some(site)).unwrap();
        indent(d, &ACCOUNT, None, fine).unwrap();
        for name in ["Read Me First", "Schedule Basis", "Change Log"] {
            let t = add_category(d, Category::Notebook, None).unwrap().unwrap();
            set_category(d, Category::Notebook, t, "memo_type", name).unwrap();
        }
        let t = categories(d, Category::Notebook)[0].id;
        set_category(d, Category::Notebook, t, "eps_flag", "N").unwrap();
    }
    let (ctx, mut app, mut painter, mut time, path) = open_app(p);
    let mut shots = vec![];
    type Shot = (&'static str, Box<dyn Fn(&mut App)>, Option<&'static str>);
    let shots_spec: [Shot; 9] = [
        (
            "headless-activity-codes.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::ACTIVITY_CODES)),
            None,
        ),
        (
            "headless-activity-code-types.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::ACTIVITY_CODES)),
            Some("codes_modify"),
        ),
        (
            "headless-activity-code-delete.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::ACTIVITY_CODES)),
            Some("codes_delete"),
        ),
        (
            "headless-project-codes.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::PROJECT_CODES)),
            None,
        ),
        (
            "headless-resource-codes.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::RESOURCE_CODES)),
            None,
        ),
        (
            "headless-udf.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::Udf)),
            None,
        ),
        (
            "headless-cost-accounts.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::CostAccounts)),
            None,
        ),
        (
            "headless-admin-categories.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::AdminCategories)),
            Some("admin_tab_notebook"),
        ),
        (
            "headless-admin-units.png",
            Box::new(|a: &mut App| a.open_dictionary(Dictionary::AdminCategories)),
            Some("admin_tab_units"),
        ),
    ];
    for (name, open, action) in shots_spec {
        open(&mut app);
        idle(&mut app, &ctx, &mut painter, &mut time);
        if let Some(action) = action {
            painter.click_action(&mut app, &ctx, &mut time, action);
            idle(&mut app, &ctx, &mut painter, &mut time);
        }
        let (image, triangles) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        assert!(triangles > 500, "{name}: {triangles}");
        image.save(artifacts.join(name)).unwrap();
        shots.push(name);
        app.dictionaries = DictionaryState::default();
        idle(&mut app, &ctx, &mut painter, &mut time);
    }
    assert_eq!(shots.len(), 9);
    drop(app);
    cleanup(path);
}

/// Cost accounts nested `depth` levels deep, read back from export text the
/// way an imported file arrives. Returns the account ids from the top down.
fn deep_accounts(depth: usize) -> (ProjectData, Vec<i64>) {
    let mut p = openxer_xer::starter();
    let d = &mut p.document;
    let first = records::next_id(d, "ACCOUNT", "acct_id");
    let ids: Vec<i64> = (0..depth as i64).map(|i| first + i).collect();
    for (i, id) in ids.iter().enumerate() {
        let parent = if i == 0 {
            String::new()
        } else {
            ids[i - 1].to_string()
        };
        records::add_row(
            d,
            "ACCOUNT",
            &[
                ("acct_id", id.to_string()),
                ("parent_acct_id", parent),
                ("acct_seq_num", "0".into()),
                ("acct_short_name", format!("A{i}")),
                ("acct_name", format!("Account {i}")),
            ],
        );
    }
    let p = reread(&p);
    (p, ids)
}

/// A crafted file can nest a dictionary thousands of levels deep: the tree,
/// the subtree and the clipboard walk it without recursion, on a small stack.
#[test]
fn deep_imported_tree_walks_without_recursion() {
    let (p, ids) = deep_accounts(10_000);
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let d = &p.document;
            let nodes = tree(d, &ACCOUNT, None);
            assert_eq!(nodes.len(), ids.len());
            for (i, n) in nodes.iter().enumerate() {
                assert_eq!((n.id, n.depth), (ids[i], i));
                assert_eq!(n.parent, i.checked_sub(1).map(|j| ids[j]));
                assert_eq!(n.has_children, i + 1 < ids.len());
            }
            assert_eq!(subtree(d, &ACCOUNT, None, ids[0]), ids);
            let copy = clip(d, &ACCOUNT, None, ids[0], false);
            assert_eq!(copy.nodes.len(), ids.len());
            assert!(copy.nodes.iter().enumerate().all(|(i, n)| n.depth == i));
            let cut = clip(d, &ACCOUNT, None, ids[5_000], true);
            assert_eq!(cut.nodes.len(), 5_000);
            assert_eq!(cut.nodes[0].depth, 0);
        })
        .unwrap();
    worker.join().unwrap();
}

/// Copy and paste of a nested branch rebuilds the same shape without
/// recursion, and the copies get new keys at the top level only.
#[test]
fn deep_copy_pastes_iteratively() {
    let (mut p, ids) = deep_accounts(300);
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let d = &mut p.document;
            let copy = clip(d, &ACCOUNT, None, ids[0], false);
            let added = paste(d, &ACCOUNT, None, Some(ids[0]), &copy).unwrap();
            assert_eq!(added.len(), 1);
            let nodes = tree(d, &ACCOUNT, None);
            assert_eq!(nodes.len(), 600);
            let at = nodes.iter().position(|n| n.id == added[0]).unwrap();
            assert_eq!(at, 300);
            assert!(nodes[at..].iter().enumerate().all(|(i, n)| n.depth == i));
            let key = |n: &ops::Node| value(d, &ACCOUNT, n.id, "acct_short_name");
            assert_eq!(key(&nodes[at]), "A02");
            assert_eq!(key(&nodes[at + 1]), "A1");
            assert_eq!(key(&nodes[599]), "A299");
        })
        .unwrap();
    worker.join().unwrap();
}

/// With a tiny length limit every numbered name soon collides; the search
/// stops with an error instead of looping, and nothing is added.
#[test]
fn unique_key_with_tiny_length_stops() {
    let mut p = openxer_xer::starter();
    let pid = p.model.project.proj_id;
    let d = &mut p.document;
    // One character: N, then 2 to 9, then nothing fits.
    let one = add_code_type(d, ACT, Scope::Project, pid, Some("One")).unwrap();
    set_code_length(d, ACT, one, 1).unwrap();
    for _ in 0..9 {
        add_code_value(d, ACT, one, None).unwrap();
    }
    let keys: Vec<String> = shorts(d, ACT, one).into_iter().map(|v| v.0).collect();
    assert_eq!(keys, ["N", "2", "3", "4", "5", "6", "7", "8", "9"]);
    let before = rows(d, "ACTVCODE");
    assert!(add_code_value(d, ACT, one, None).is_err());
    assert_eq!(rows(d, "ACTVCODE"), before);
    // Two characters: the number replaces the end of the name.
    let two = add_code_type(d, ACT, Scope::Project, pid, Some("Two")).unwrap();
    set_code_length(d, ACT, two, 2).unwrap();
    for _ in 0..99 {
        add_code_value(d, ACT, two, None).unwrap();
    }
    let keys: Vec<String> = shorts(d, ACT, two).into_iter().map(|v| v.0).collect();
    assert_eq!(&keys[..3], ["Ne", "N2", "N3"]);
    assert_eq!(&keys[8..10], ["N9", "10"]);
    assert_eq!(keys.last().unwrap(), "99");
    assert!(keys.iter().all(|k| k.chars().count() <= 2));
    let distinct: std::collections::BTreeSet<String> =
        keys.iter().map(|k| k.to_lowercase()).collect();
    assert_eq!(distinct.len(), keys.len());
    assert!(add_code_value(d, ACT, two, None).is_err());
    // A paste that runs out of names leaves the document as it was.
    let copy = clip(
        d,
        &value_spec(d, ACT, one),
        Some(one),
        first_value(d, one),
        false,
    );
    let before = d.text();
    assert!(paste(d, &value_spec(d, ACT, one), Some(one), None, &copy).is_err());
    assert_eq!(d.text(), before);
}

fn first_value(doc: &Document, type_id: i64) -> i64 {
    code_values(doc, ACT, type_id)[0].id
}

/// Renaming a user defined field keeps it in every layout that uses it:
/// saved layouts (and the bottom table kept with them), the bottom table
/// now shown, and user filters, not only the working layout.
#[test]
fn udf_rename_fixes_every_layout_and_filter_that_uses_it() {
    use crate::grid::filters::{Criterion, UserFilter};
    use crate::grid::layout::{ColumnSetting, GroupLevel, SortKey};
    let (_ctx, mut app, _painter, _time, path) = open_app(openxer_xer::starter());
    let id = add_udf(&mut app.project.document, "TASK");
    set_udf_title(&mut app.project.document, id, "Crew").unwrap();
    // A saved layout grouped, sorted and listing the field, with the field
    // in its bottom table too.
    app.grid
        .layout
        .columns
        .push(ColumnSetting::new("udf:Crew", 80.));
    app.panes.grid.columns = vec![ColumnSetting::new("udf:Crew", 60.)];
    app.save_layout_as("Crews").unwrap();
    let saved = app
        .grid
        .library
        .iter_mut()
        .find(|l| l.name == "Crews")
        .unwrap();
    saved.group_sort.levels.push(GroupLevel {
        column: "udf:Crew".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    });
    saved.group_sort.sort.push(SortKey {
        column: "udf:Crew".into(),
        descending: false,
    });
    // The working layout is another one that does not use it.
    app.open_layout(crate::grid::layout::STANDARD_NAME, true)
        .unwrap();
    app.panes.grid.columns = vec![ColumnSetting::new("udf:Crew", 60.)];
    app.grid.user_filters.push(UserFilter {
        name: "Big crews".into(),
        match_all: true,
        rows: vec![Criterion {
            column: "udf:Crew".into(),
            ..Default::default()
        }],
    });
    app.run_dictionary(Cmd::UdfTitle(id, "Crew Size".into()))
        .unwrap();
    let saved = app.grid.library.iter().find(|l| l.name == "Crews").unwrap();
    assert!(saved.columns.iter().any(|c| c.id == "udf:Crew Size"));
    assert_eq!(
        saved.group_sort.levels.last().unwrap().column,
        "udf:Crew Size"
    );
    assert_eq!(
        saved.group_sort.sort.last().unwrap().column,
        "udf:Crew Size"
    );
    let bottom = saved.panes.as_ref().unwrap()["bottom_columns"].to_string();
    assert!(
        bottom.contains("udf:Crew Size") && !bottom.contains("\"udf:Crew\""),
        "{bottom}"
    );
    assert_eq!(app.panes.grid.columns[0].id, "udf:Crew Size");
    assert_eq!(
        app.grid.user_filters.last().unwrap().rows[0].column,
        "udf:Crew Size"
    );
    // Kept in the workspace: a reopened library has the new name.
    let stored = app
        .store
        .preference(crate::grid::LIBRARY_KEY)
        .unwrap()
        .unwrap();
    assert!(stored.contains("udf:Crew Size"));
    drop(app);
    cleanup(path);
}

use super::*;

fn workspace() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    crate::storage::schema(&c).unwrap();
    ensure(&c).unwrap();
    c.execute(
        "INSERT INTO preferences(key,value) VALUES(?1,'3')",
        [SCHEMA_KEY],
    )
    .unwrap();
    c
}

fn insert(c: &Connection, source: &str, text: &str) -> i64 {
    c.execute(
        "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES(?1,1,'P',?2)",
        params![source, text],
    )
    .unwrap();
    c.last_insert_rowid()
}

#[test]
fn schema_is_1_without_the_preference() {
    let c = Connection::open_in_memory().unwrap();
    assert_eq!(schema(&c).unwrap(), 1);
    crate::storage::schema(&c).unwrap();
    assert_eq!(schema(&c).unwrap(), 1);
    assert_eq!(schema(&workspace()).unwrap(), 3);
}

#[test]
fn store_then_hydrate_returns_the_project() {
    let c = workspace();
    let p = openxer_xer::starter();
    let doc = p.updated_document();
    let stored = store(&c, &doc, None, None).unwrap();
    assert!(!stored.text.contains("Standard 5-day"));
    let id = insert(&c, "A.xer", &stored.text);
    let (dict, revision) = read_dictionary(&c).unwrap();
    assert_eq!(revision, 1);
    assert!(dict.table("CALENDAR").is_some());
    let text: String = c
        .query_row("SELECT xer_content FROM projects WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    let mut back = Document::parse(&text).unwrap();
    let mark = hydrate(&c, &mut back, None).unwrap();
    assert_eq!(back.text(), doc.text());
    assert_eq!(mark.revision, 1);
    assert!(mark.rows.contains(&("CALENDAR".to_string(), 1)));
    let keys = read_keys(&c).unwrap();
    assert!(keys["CALENDAR"] >= 1);
    assert_eq!(back.id_floor_of("CALENDAR"), keys["CALENDAR"]);
    // Saving the same project again changes nothing.
    let again = store(&c, &back, Some(&mark), Some(id)).unwrap();
    assert_eq!(again.text, stored.text);
    assert_eq!(read_dictionary(&c).unwrap().1, 1);
}

#[test]
fn commands_read_and_write_a_shared_dictionary_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    let doc = openxer_xer::starter().updated_document();
    {
        let c = Connection::open(&path).unwrap();
        crate::storage::schema(&c).unwrap();
        start_new(&c).unwrap();
        let id = insert(&c, "A.xer", "");
        let stored = store(&c, &doc, None, Some(id)).unwrap();
        c.execute(
            "UPDATE projects SET xer_content=?1 WHERE id=?2",
            params![stored.text, id],
        )
        .unwrap();
    }
    // load reads the project whole, its calendars from the dictionary.
    let loaded = crate::storage::load(&path, None, None).unwrap();
    assert_eq!(loaded.project.updated_document().text(), doc.text());
    assert!(loaded.mark.is_some());
    let inspected = crate::storage::inspect(&path).unwrap();
    assert_eq!(inspected["workspace_schema"], 3);
    assert_eq!(inspected["dictionary"]["rows"]["CALENDAR"], 1);
    // A newer schema is refused.
    let c = Connection::open(&path).unwrap();
    c.execute(
        "UPDATE preferences SET value='99' WHERE key=?1",
        [SCHEMA_KEY],
    )
    .unwrap();
    drop(c);
    let e = crate::storage::load(&path, None, None).err().unwrap();
    assert_eq!(e.code, "workspace_schema_unsupported");
}

fn code_types(doc: &Document) -> Vec<String> {
    doc.table("ACTVTYPE").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| t.value(r, "actv_code_type").to_string())
            .collect()
    })
}

#[test]
fn eps_codes_are_shared_with_the_projects_under_their_eps_node() {
    let c = workspace();
    let starter = openxer_xer::starter().updated_document().text();
    let p = insert(&c, "P.xer", &starter);
    let q = insert(&c, "Q.xer", &starter);
    let r = insert(&c, "R.xer", &starter);
    // Workspace > North (P, Q) and South (R).
    let mut ws = Workspace::default();
    let root = ws.eps()[0].id;
    let north = ws.add_folder(Some(root), "North", Some("N")).unwrap();
    let south = ws.add_folder(Some(root), "South", Some("S")).unwrap();
    ws.place(p, north).unwrap();
    ws.place(q, north).unwrap();
    ws.place(r, south).unwrap();
    c.execute(
        "INSERT INTO preferences(key,value) VALUES(?1,?2)",
        params![openxer_xer::workspace::KEY, ws.to_text()],
    )
    .unwrap();
    // P brings an EPS code filed under its source database's node 999, and
    // a global one.
    let mut project = openxer_xer::starter();
    openxer_xer::edit::tables::add_code_type(&mut project, "Unit", "eps", None).unwrap();
    openxer_xer::edit::tables::add_code_type(&mut project, "Area", "global", None).unwrap();
    let doc = project.updated_document();
    let stored = store(&c, &doc, None, Some(p)).unwrap();
    assert_eq!(stored.placed, 1, "the EPS code moves to P's node");
    assert!(stored.changed_document());
    let (dict, _) = read_dictionary(&c).unwrap();
    let t = dict.table("ACTVTYPE").unwrap();
    let unit = t
        .rows
        .iter()
        .find(|row| t.value(row, "actv_code_type") == "Unit")
        .unwrap();
    assert_eq!(t.value(unit, "wbs_id"), north.to_string());
    // Q (same node) and P see both codes; R (another node) the global one.
    for (row, want) in [
        (p, vec!["Unit", "Area"]),
        (q, vec!["Unit", "Area"]),
        (r, vec!["Area"]),
    ] {
        let mut back = Document::parse(&stored.text).unwrap();
        hydrate(&c, &mut back, Some(row)).unwrap();
        let mut got = code_types(&back);
        got.sort();
        let mut want: Vec<String> = want.into_iter().map(String::from).collect();
        want.sort();
        assert_eq!(got, want, "workspace row {row}");
    }
    // A project under a node below North sees North's codes too.
    let mut ws = read_eps(&c).unwrap();
    let below = ws.add_folder(Some(north), "Unit 1", Some("U1")).unwrap();
    ws.place(r, below).unwrap();
    c.execute(
        "UPDATE preferences SET value=?2 WHERE key=?1",
        params![openxer_xer::workspace::KEY, ws.to_text()],
    )
    .unwrap();
    let mut back = Document::parse(&stored.text).unwrap();
    hydrate(&c, &mut back, Some(r)).unwrap();
    assert!(code_types(&back).contains(&"Unit".to_string()));
}

/// Schema marker: the command line refuses a schema 2 workspace an earlier
/// build made (its EPS codes are still in each project's text) instead of
/// reading it as this build's layout, and `migrate` upgrades it.
#[test]
fn an_earlier_shared_dictionary_workspace_is_refused_until_upgraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    let doc = openxer_xer::starter().updated_document();
    {
        let c = Connection::open(&path).unwrap();
        crate::storage::schema(&c).unwrap();
        ensure(&c).unwrap();
        c.execute(
            "INSERT INTO preferences(key,value) VALUES(?1,?2)",
            params![SCHEMA_KEY, EARLIER_DICTIONARY_SCHEMA.to_string()],
        )
        .unwrap();
        insert(&c, "A.xer", &doc.text());
    }
    let e = crate::storage::load(&path, None, None).err().unwrap();
    assert_eq!(e.code, "workspace_needs_migration");
    let mut c = Connection::open(&path).unwrap();
    let report = migrate(&mut c, false, None).unwrap();
    assert_eq!(report["schema_before"], 2);
    assert_eq!(report["schema_after"], DICTIONARY_SCHEMA);
    drop(c);
    let loaded = crate::storage::load(&path, None, None).unwrap();
    assert_eq!(loaded.project.updated_document().text(), doc.text());
}

/// A project with the starter's rows and the notebook topics `topics`.
fn with_topics(source_key: &str, topics: &[(i64, &str)]) -> Document {
    let mut doc = openxer_xer::starter().updated_document();
    for (id, name) in topics {
        doc.table_mut("MEMOTYPE").add(&[
            ("memo_type_id", id.to_string()),
            ("seq_num", id.to_string()),
            ("memo_type", name.to_string()),
        ]);
    }
    let t = doc.table_mut("PROJECT");
    t.set(0, "proj_short_name", source_key.into());
    doc
}

/// A dictionary row the sample brought stays when another project brings
/// the same row (same name and content), even one that project does not
/// use; the rows only the sample has go.
#[test]
fn sample_rows_another_project_shares_stay() {
    let c = workspace();
    let sample = insert(&c, "Sample", "");
    let before = read_dictionary(&c).unwrap().0;
    let doc = with_topics("RIVERBEND", &[(1, "Scope"), (2, "Field Notes")]);
    let stored = store(&c, &doc, None, Some(sample)).unwrap();
    c.execute(
        "UPDATE projects SET xer_content=?1 WHERE id=?2",
        params![stored.text, sample],
    )
    .unwrap();
    record_sample(&c, sample, &before).unwrap();
    let record = read_sample(&c).unwrap().unwrap();
    assert!(record.rows.contains(&("MEMOTYPE".to_string(), 1)));
    assert!(record.rows.contains(&("CALENDAR".to_string(), 1)));
    // Another project brings the same "Scope" topic and calendar.
    let other = insert(&c, "Mine.xer", "");
    let mine = with_topics("MINE", &[(1, "Scope")]);
    let stored = store(&c, &mine, None, Some(other)).unwrap();
    c.execute(
        "UPDATE projects SET xer_content=?1 WHERE id=?2",
        params![stored.text, other],
    )
    .unwrap();
    let record = read_sample(&c).unwrap().unwrap();
    assert!(!record.rows.contains(&("MEMOTYPE".to_string(), 1)));
    assert!(!record.rows.contains(&("CALENDAR".to_string(), 1)));
    c.execute("DELETE FROM projects WHERE id=?1", [sample])
        .unwrap();
    let removal = plan_sample_removal(&c, sample, &[]).unwrap().unwrap();
    assert_eq!(removal.removed, 1, "only Field Notes goes");
    let (mut dict, _) = read_dictionary(&c).unwrap();
    removal.delta.apply(&mut dict, true);
    let t = dict.table("MEMOTYPE").unwrap();
    let topics: Vec<&str> = t.rows.iter().map(|r| t.value(r, "memo_type")).collect();
    assert_eq!(topics, vec!["Scope"]);
    assert!(dict.table("CALENDAR").is_some_and(|t| t.rows.len() == 1));
    // Another workspace row is no sample.
    assert!(plan_sample_removal(&c, other, &[]).unwrap().is_none());
}

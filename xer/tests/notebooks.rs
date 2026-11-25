//! Project and WBS notebooks and project code assignments through the
//! shared edit layer, and their file round trip: what is added, edited and
//! deleted in `WBSMEMO` (the project's notebook sits on its own WBS node)
//! and `PROJPCAT` is what an XER written and read again holds.
use openxer_xer::edit::notebooks::{self, Owner};
use openxer_xer::edit::project_codes;
use openxer_xer::edit::records::{self, num};
use openxer_xer::edit::rich_memo::{Format, LineKind, RichMemo};
use openxer_xer::{starter, Document, ProjectData};

fn node_ids(p: &ProjectData) -> (i64, i64) {
    let t = p.document.table("PROJWBS").unwrap();
    let pid = p.model.project.proj_id;
    let mine = |r: &&Vec<String>| num(t.value(r, "proj_id")) == pid;
    let project = t
        .rows
        .iter()
        .filter(mine)
        .find(|r| t.value(r, "proj_node_flag") == "Y")
        .map(|r| num(t.value(r, "wbs_id")))
        .unwrap();
    let element = t
        .rows
        .iter()
        .filter(mine)
        .find(|r| t.value(r, "proj_node_flag") != "Y")
        .map(|r| num(t.value(r, "wbs_id")))
        .unwrap();
    (project, element)
}

fn with_dictionaries() -> ProjectData {
    let mut p = starter();
    let doc = &mut p.document;
    // The starter has no project node: give it one over its elements.
    let wbs = doc.table_mut("PROJWBS");
    for r in 0..wbs.rows.len() {
        wbs.set(r, "parent_wbs_id", "5".into());
        wbs.set(r, "proj_node_flag", "N".into());
    }
    wbs.add(&[
        ("wbs_id", "5".into()),
        ("proj_id", p.model.project.proj_id.to_string()),
        ("wbs_short_name", "RIVERBEND".into()),
        ("wbs_name", "Riverbend".into()),
        ("seq_num", "0".into()),
        ("proj_node_flag", "Y".into()),
    ]);
    for (id, name, proj, wbs) in [(901, "Scope", "Y", "Y"), (902, "Risks", "Y", "N")] {
        records::add_row(
            doc,
            "MEMOTYPE",
            &[
                ("memo_type_id", id.to_string()),
                ("seq_num", id.to_string()),
                ("eps_flag", "N".into()),
                ("proj_flag", proj.into()),
                ("wbs_flag", wbs.into()),
                ("task_flag", "Y".into()),
                ("memo_type", name.into()),
            ],
        );
    }
    records::add_row(
        doc,
        "PCATTYPE",
        &[
            ("proj_catg_type_id", "31".into()),
            ("seq_num", "1".into()),
            ("proj_catg_short_len", "10".into()),
            ("proj_catg_type", "Region".into()),
        ],
    );
    for (id, short, name) in [(310, "N", "North"), (311, "S", "South")] {
        records::add_row(
            doc,
            "PCATVAL",
            &[
                ("proj_catg_id", id.to_string()),
                ("proj_catg_type_id", "31".into()),
                ("seq_num", id.to_string()),
                ("proj_catg_short_name", short.into()),
                ("proj_catg_name", name.into()),
            ],
        );
    }
    p
}

fn reread(p: &ProjectData) -> ProjectData {
    let text = p.updated_document().text();
    let pid = p.model.project.proj_id;
    ProjectData::from_document(Document::parse(&text).unwrap(), pid, "again".into()).unwrap()
}

#[test]
fn project_and_wbs_notebooks_survive_the_file() {
    let mut p = with_dictionaries();
    let pid = p.model.project.proj_id;
    let (node, element) = node_ids(&p);
    let project = Owner::Project(node);
    let wbs = Owner::Wbs(element);
    // Topics are offered by their Project and WBS flags.
    let offered = |p: &ProjectData, o| notebooks::unused_topics(&p.document, pid, o);
    assert_eq!(offered(&p, project).len(), 2);
    assert_eq!(offered(&p, wbs), vec![(901, "Scope".to_string())]);

    let scope = notebooks::add(&mut p.document, pid, project, 901).unwrap();
    let risks = notebooks::add(&mut p.document, pid, project, 902).unwrap();
    assert!(notebooks::add(&mut p.document, pid, project, 901).is_err());
    let on_wbs = notebooks::add(&mut p.document, pid, wbs, 901).unwrap();
    let mut memo = RichMemo::plain("Tie-ins during the outage\nCrane on site\nHydrotest");
    memo.toggle(0, 7, Format::Bold);
    memo.toggle_lines(26, 40, LineKind::Bullet);
    let html = memo.to_html();
    assert!(notebooks::set_text(&mut p.document, project, scope, &html).unwrap());
    assert!(!notebooks::set_text(&mut p.document, project, scope, &html).unwrap());
    notebooks::set_text(
        &mut p.document,
        wbs,
        on_wbs,
        "<html><body>WBS note</body></html>",
    )
    .unwrap();
    notebooks::delete(&mut p.document, project, risks).unwrap();

    let back = reread(&p);
    let project_memos = notebooks::memos(&back.document, pid, project);
    assert_eq!(project_memos.len(), 1, "the deleted topic stays deleted");
    assert_eq!(project_memos[0].topic, "Scope");
    assert_eq!(project_memos[0].text, html);
    assert_eq!(RichMemo::parse(&project_memos[0].text), memo);
    let wbs_memos = notebooks::memos(&back.document, pid, wbs);
    assert_eq!(wbs_memos.len(), 1);
    assert_eq!(wbs_memos[0].text, "<html><body>WBS note</body></html>");
}

#[test]
fn project_code_assignments_survive_the_file() {
    let mut p = with_dictionaries();
    let pid = p.model.project.proj_id;
    assert!(project_codes::assign(&mut p.document, pid, 310).unwrap());
    assert!(!project_codes::assign(&mut p.document, pid, 310).unwrap());
    // A second value of the same type replaces the first.
    assert!(project_codes::assign(&mut p.document, pid, 311).unwrap());
    let back = reread(&p);
    let codes = project_codes::assigned(&back.document, pid);
    assert_eq!(codes.len(), 1);
    assert_eq!(
        (codes[0].code_type.as_str(), codes[0].value.as_str()),
        ("Region", "S")
    );
    assert_eq!(codes[0].description, "South");

    let mut p = back;
    assert!(project_codes::remove(&mut p.document, pid, 311));
    assert!(!project_codes::remove(&mut p.document, pid, 311));
    assert!(project_codes::assigned(&reread(&p).document, pid).is_empty());

    // By name, as the command line names them.
    project_codes::set_project_code(&mut p, "Region", Some("N")).unwrap();
    assert_eq!(project_codes::assigned(&p.document, pid)[0].value, "N");
    assert!(project_codes::set_project_code(&mut p, "Region", Some("X")).is_err());
    assert!(project_codes::set_project_code(&mut p, "Area", Some("N")).is_err());
    project_codes::set_project_code(&mut p, "Region", None).unwrap();
    assert!(project_codes::assigned(&p.document, pid).is_empty());
}

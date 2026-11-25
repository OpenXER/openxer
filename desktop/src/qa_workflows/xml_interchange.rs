//! Workflow: File > Export and File > Import in the project XML format
//! (backlog T2.7). The sample project goes out through the Export wizard as
//! XML, the file holds what the project holds, it comes back in through the
//! Import wizard as a new project with the same activities, relationships
//! and assignments, and Undo Last Import takes the import back.
use super::*;

fn sample(name: &str) -> Driver {
    Driver::new(name, openxer_xer::sample_project::sample_project().current)
}

/// Activity IDs with names, relationship and assignment counts.
fn content(p: &ProjectData) -> (Vec<String>, usize, usize) {
    let mut names: Vec<String> = p
        .model
        .activities
        .iter()
        .map(|a| format!("{} {}", a.task_code, p.details[&a.task_id].name))
        .collect();
    names.sort();
    let assignments = p.document.table("TASKRSRC").map_or(0, |t| t.rows.len());
    (names, p.model.links.len(), assignments)
}

/// File > Import of one XML file through every page (Create New).
fn import_xml(d: &mut Driver, path: &std::path::Path) {
    d.home();
    d.click("menu_bar_file");
    d.click("file_import");
    d.click("import_format_xml");
    d.click("xer_import_next");
    let xml = d.texts().iter().any(|(t, _)| t.contains("XML files"));
    assert!(xml, "the file page offers XML files");
    d.click("xer_import_file");
    d.type_text(&path.display().to_string());
    d.click("xer_import_next");
    let w = d.app.interchange.xer.as_ref().unwrap();
    assert!(w.error.is_none(), "{:?}", w.error);
    assert_eq!(w.step, 2, "the projects of the XML file are listed");
    assert_eq!(w.rows.len(), 1);
    d.click("xer_import_next");
    let w = d.app.interchange.xer.as_ref().unwrap();
    assert!(w.error.is_none(), "{:?}", w.error);
    assert_eq!(w.step, 4);
    d.click("xer_import_finish");
    assert!(d.app.interchange.xer.is_none(), "the import finished");
    d.click("interchange_message_ok");
    d.frames(2);
}

#[test]
fn workflow_project_xml_export_then_import_and_undo() {
    let mut d = sample("wf-xml-interchange");
    let original = content(&d.app.project);
    let path = d.dir.join("sample.xml");
    d.export(interchange::ExportFormat::Xml, &path);
    let text = std::fs::read_to_string(&path).unwrap();
    let root = format!("<{} xmlns=", openxer_interchange::project_xml::ROOT);
    assert!(text.contains(&root), "an XML project file");
    // The first two lines as the reference writes them.
    use openxer_interchange::project_xml::{root_tag, DECLARATION};
    let head = format!("{DECLARATION}\r\n{}\r\n", root_tag());
    assert!(text.starts_with(&head), "the observed root line");
    assert!(text.contains("<Relationship>") && text.contains("<ResourceAssignment>"));
    // The file holds the project as it is open.
    let doc = openxer_interchange::project_xml::read_file(&path).unwrap();
    let key = d.app.project.model.project.proj_id;
    let back = ProjectData::from_document(doc, key, "xml".into()).unwrap();
    assert_eq!(content(&back), original, "the XML file holds the project");

    // In again, as a new project.
    let ids = |d: &Driver| -> Vec<i64> {
        interchange::workspace::rows(&d.app.store)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect()
    };
    let before = ids(&d);
    import_xml(&mut d, &path);
    let after = ids(&d);
    assert_eq!(after.len(), before.len() + 1, "Create New adds a project");
    let imported = after
        .into_iter()
        .find(|id| !before.contains(id))
        .expect("the imported project is in the workspace");
    let stored = d.app.store.load(imported).unwrap();
    assert_eq!(content(&stored), original, "the import has every activity");

    // Undo Last Import takes it back.
    d.home();
    d.click("menu_bar_file");
    d.click("file_import");
    d.click("undo_last_import");
    d.frames(2);
    d.click("undo_import_undo import");
    d.frames(2);
    if d.app.interchange.message.is_some() {
        d.click("interchange_message_ok");
    }
    if d.app.interchange.xer.is_some() {
        d.click("xer_import_cancel");
    }
    d.frames(2);
    assert_eq!(ids(&d), before, "the imported project is gone");
    assert!(d.app.store.load(imported).is_err());
}

//! Workflow: Global Change follow-ups. Deleting a line in Modify Global
//! Change asks first; a pasted copy takes the next free number; the report
//! shows Selected, Project ID, WBS Code, Activity ID, Field, Old Value, New
//! Value and Exception; Clear All then two ticked rows commits only those,
//! as one undo step; Save Report writes the same rows to a text file and,
//! with As HTML, to an HTML page.
use super::*;
use crate::editing::gc_report::COLUMNS;

/// The open report's lines (task id, ticked).
fn ticks(d: &Driver) -> Vec<(i64, bool)> {
    d.app
        .editing
        .dialogs
        .gc_report
        .as_ref()
        .expect("the report is open")
        .changes
        .iter()
        .map(|c| (c.task_id, c.apply))
        .collect()
}

/// Pick saved global change `name` in the list and click Apply Change.
fn apply(d: &mut Driver, name: &str) {
    let k = d
        .app
        .editing
        .global_changes
        .iter()
        .position(|g| g.name == name)
        .expect("saved");
    d.click(&format!("gc_row_{k}"));
    d.click("gc_apply");
    assert!(
        d.app.editing.dialogs.gc_report.is_some(),
        "Apply Change opens the report"
    );
}

/// Every activity's Original Duration by task id.
fn durations(d: &Driver) -> Vec<(i64, i64)> {
    d.app
        .project
        .model
        .activities
        .iter()
        .map(|a| {
            (
                a.task_id,
                d.app.project.details[&a.task_id].original_duration,
            )
        })
        .collect()
}

#[test]
fn workflow_global_change_report_selection_saved_file_paste_and_delete_confirm() {
    let mut d = fresh("qa_gc_report");
    let before = durations(&d);
    d.home();
    d.click("menu_bar_tools");
    d.click("tools_global_change");
    assert!(
        d.app.editing.dialogs.gc_list.is_some(),
        "Global Change opens"
    );
    d.click("gc_new");
    type_into(&mut d, "gc_edit_name", "Double durations");

    // Delete asks first: No keeps the line, Escape keeps it, Yes removes it.
    d.click("gc_if_0");
    d.click("gc_edit_delete");
    assert!(d.has("gc_delete_line_yes") && d.has("gc_delete_line_no"));
    assert!(
        d.app
            .editing
            .dialogs
            .gc_edit
            .as_ref()
            .unwrap()
            .confirm_delete
    );
    d.click("gc_delete_line_no");
    let e = d.app.editing.dialogs.gc_edit.as_ref().unwrap();
    assert!(!e.confirm_delete, "No closes the box");
    assert_eq!(e.gc.conditions.len(), 1, "No keeps the line");
    d.click("gc_edit_delete");
    d.key(Key::Escape);
    d.frames(2);
    let e = d.app.editing.dialogs.gc_edit.as_ref();
    assert!(
        e.is_some_and(|e| !e.confirm_delete && e.gc.conditions.len() == 1),
        "Escape closes the box only and keeps the line"
    );
    d.click("gc_edit_delete");
    d.click("gc_delete_line_yes");
    {
        let e = d.app.editing.dialogs.gc_edit.as_mut().unwrap();
        assert!(e.gc.conditions.is_empty(), "Yes removes the line");
        assert!(!e.confirm_delete);
        // Parameter and operator are drop-downs; pick them as a click in
        // their lists would.
        e.gc.then[0].column = "original_duration".into();
        e.gc.then[0].operator = "x".into();
        e.gc.then[0].value2 = "2".into();
    }
    type_into(&mut d, "gc_Then_value_0", "Original Duration");
    d.click("gc_edit_ok");
    assert!(d.app.editing.dialogs.gc_edit.is_none(), "{:?}", d.app.error);

    // Copy, Paste: "Double durations1", then "Double durations2".
    let names = |d: &Driver| -> Vec<String> {
        d.app
            .editing
            .global_changes
            .iter()
            .map(|g| g.name.clone())
            .collect()
    };
    d.click("gc_copy");
    d.click("gc_paste");
    d.click("gc_paste");
    let list = names(&d);
    assert!(
        list.contains(&"Double durations1".to_string())
            && list.contains(&"Double durations2".to_string()),
        "{list:?}"
    );

    // The report: header, columns in order, Project ID and WBS Code filled.
    apply(&mut d, "Double durations");
    {
        let r = d.app.editing.dialogs.gc_report.as_ref().unwrap();
        assert_eq!(
            COLUMNS,
            [
                "Selected",
                "Project ID",
                "WBS Code",
                "Activity ID",
                "Field",
                "Old Value",
                "New Value",
                "Exception"
            ]
        );
        assert_eq!(r.name, "Double durations");
        assert!(!r.projects.is_empty() && !r.date.is_empty());
        assert!(r.changes.len() >= 3, "{:#?}", r.changes);
        assert!(r
            .changes
            .iter()
            .all(|c| !c.project.is_empty() && !c.wbs.is_empty() && c.title == "Original Duration"));
        assert!(
            r.file.save && r.file.overwrite && !r.file.html,
            "observed defaults"
        );
    }
    for key in [
        "gc_report_select_all",
        "gc_report_clear_all",
        "gc_report_save",
        "gc_report_path",
        "gc_report_html",
        "gc_report_overwrite",
        "gc_save_report",
        "gc_cancel_changes",
        "gc_commit",
        "gc_report_help",
    ] {
        assert!(d.has(key), "{key} is drawn");
    }
    // Bottom buttons left to right: Help first (as in every bottom row),
    // then Save to File..., Cancel Changes, Commit Changes.
    let x = |d: &Driver, k: &str| d.rect(k).left();
    assert!(x(&d, "gc_report_help") < x(&d, "gc_save_report"));
    assert!(x(&d, "gc_save_report") < x(&d, "gc_cancel_changes"));
    assert!(x(&d, "gc_cancel_changes") < x(&d, "gc_commit"));

    // Clear All, then tick two rows.
    d.click("gc_report_clear_all");
    assert!(ticks(&d).iter().all(|(_, on)| !on), "Clear All clears");
    d.click("gc_report_select_all");
    assert!(ticks(&d).iter().all(|(_, on)| *on), "Select All ticks");
    d.click("gc_report_clear_all");
    d.click("gc_report_check_0");
    d.click("gc_report_check_2");
    let picked: Vec<i64> = ticks(&d)
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(picked.len(), 2);

    // Save Report to a file in this test's own folder.
    let text_path = d.dir.join("gc-report.txt");
    type_into(&mut d, "gc_report_path", &text_path.display().to_string());
    let expected = {
        let r = d.app.editing.dialogs.gc_report.as_ref().unwrap();
        assert_eq!(r.file.path, text_path.display().to_string());
        r.text()
    };
    let undo = d.app.history.counts().0;
    d.click("gc_commit");
    d.assert_clean("commit");
    assert!(
        d.app.editing.dialogs.gc_report.is_none(),
        "Commit closes it"
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    let after = durations(&d);
    for ((id, old), (_, new)) in before.iter().zip(&after) {
        if picked.contains(id) {
            assert_eq!(*new, 2 * old, "task {id} doubled");
        } else {
            assert_eq!(new, old, "task {id} untouched");
        }
    }
    let written = std::fs::read_to_string(&text_path).expect("Save Report wrote the file");
    assert_eq!(written, expected, "the file holds the report's rows");
    let lines: Vec<&str> = written.lines().collect();
    assert!(lines[0].starts_with("Global Change\tDouble durations"));
    assert_eq!(lines[4], COLUMNS.join("\t"));
    assert_eq!(
        lines[5..].iter().filter(|l| l.starts_with("Yes\t")).count(),
        2,
        "two selected rows"
    );

    // Again as HTML: the options are remembered, As HTML switches the
    // extension, the page holds the same rows.
    if d.app.editing.dialogs.gc_list.is_none() {
        d.click("menu_bar_tools");
        d.click("tools_global_change");
    }
    apply(&mut d, "Double durations");
    assert_eq!(
        d.app.editing.dialogs.gc_report.as_ref().unwrap().file.path,
        text_path.display().to_string(),
        "the path is remembered"
    );
    d.click("gc_report_html");
    let html_path = d.dir.join("gc-report.html");
    d.click("gc_report_clear_all");
    d.click("gc_report_check_1");
    let expected = {
        let r = d.app.editing.dialogs.gc_report.as_ref().unwrap();
        assert!(r.file.html);
        assert_eq!(r.file.path, html_path.display().to_string());
        r.html()
    };
    d.click("gc_commit");
    d.assert_clean("commit as HTML");
    let page = std::fs::read_to_string(&html_path).expect("the HTML report is written");
    assert_eq!(page, expected, "the page holds the report's rows");
    assert!(page.contains("<th>Exception</th>") && page.contains("Double durations"));
    assert_eq!(page.matches("<td>Yes</td>").count(), 1);
    assert_eq!(d.app.history.counts().0, undo + 2);

    // Undo twice puts every duration back.
    d.click("undo");
    d.click("undo");
    assert_eq!(durations(&d), before, "undo restores the durations");
    assert_eq!(d.app.history.counts().0, undo);
}

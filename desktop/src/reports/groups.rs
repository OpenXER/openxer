//! Report groups and batch runs. Groups form a tree saved in the workspace
//! preferences (`report_groups_v1`); each saved report names its group by
//! id. A batch run writes the chosen saved reports to one folder.
use super::doc::Output;
use super::wizard::Definition;
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};

pub const GROUPS_KEY: &str = "report_groups_v1";
/// Preference holding the last batch output folder.
pub const BATCH_FOLDER_KEY: &str = "report_batch_folder_v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
}

pub fn groups_to_json(list: &[Group]) -> String {
    Json::Array(
        list.iter()
            .map(|g| json!({"id": g.id, "name": g.name, "parent": g.parent}))
            .collect(),
    )
    .to_string()
}

/// Saved groups; entries that do not read, repeat an id or point at a
/// missing parent (or into a loop) are dropped or moved to the top level.
pub fn groups_from_json(text: &str) -> Vec<Group> {
    let mut out: Vec<Group> = vec![];
    for v in serde_json::from_str::<Json>(text)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        let Some(id) = v["id"].as_u64().and_then(|i| u32::try_from(i).ok()) else {
            continue;
        };
        if out.iter().any(|g| g.id == id) {
            continue;
        }
        out.push(Group {
            id,
            name: v["name"].as_str().unwrap_or("").to_string(),
            parent: v["parent"].as_u64().and_then(|i| u32::try_from(i).ok()),
        });
    }
    let ids: Vec<u32> = out.iter().map(|g| g.id).collect();
    for k in 0..out.len() {
        let mut seen = vec![out[k].id];
        let mut at = out[k].parent;
        while let Some(p) = at {
            if !ids.contains(&p) || seen.contains(&p) {
                out[k].parent = None;
                break;
            }
            seen.push(p);
            at = out.iter().find(|g| g.id == p).and_then(|g| g.parent);
        }
    }
    out
}

/// One row of the groups tree: a group or a saved report (index into the
/// saved list), with its depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    Group(u32),
    Report(usize),
}

/// The tree in display order: each group, then its child groups, then its
/// reports; reports without a (known) group come last at the top level.
#[cfg(test)]
pub fn outline(groups: &[Group], reports: &[Definition]) -> Vec<(Node, usize)> {
    let of: Vec<Option<u32>> = reports.iter().map(|r| r.group).collect();
    outline_by(groups, &of)
}

/// `outline` for any list of reports, given each one's group.
pub fn outline_by(groups: &[Group], member_groups: &[Option<u32>]) -> Vec<(Node, usize)> {
    fn walk(
        groups: &[Group],
        members: &[Option<u32>],
        parent: Option<u32>,
        level: usize,
        out: &mut Vec<(Node, usize)>,
    ) {
        for g in groups.iter().filter(|g| g.parent == parent) {
            out.push((Node::Group(g.id), level));
            walk(groups, members, Some(g.id), level + 1, out);
        }
        for (i, group) in members.iter().enumerate() {
            let known = group.filter(|id| groups.iter().any(|g| g.id == *id));
            if known == parent {
                out.push((Node::Report(i), level));
            }
        }
    }
    let mut out = vec![];
    walk(groups, member_groups, None, 0, &mut out);
    out
}

/// Adds a group named `name` under `parent`; returns its id.
pub fn add_group(groups: &mut Vec<Group>, parent: Option<u32>, name: &str) -> u32 {
    let id = groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
    groups.push(Group {
        id,
        name: name.to_string(),
        parent: parent.filter(|p| groups.iter().any(|g| g.id == *p)),
    });
    id
}

/// Deletes a group: its child groups and reports move to its parent.
#[cfg(test)]
pub fn delete_group(groups: &mut Vec<Group>, reports: &mut [Definition], id: u32) {
    let mut of: Vec<Option<u32>> = reports.iter().map(|r| r.group).collect();
    delete_group_by(groups, &mut of, id);
    for (r, g) in reports.iter_mut().zip(of) {
        r.group = g;
    }
}

/// `delete_group` for any list of reports, given each one's group.
pub fn delete_group_by(groups: &mut Vec<Group>, member_groups: &mut [Option<u32>], id: u32) {
    let Some(at) = groups.iter().position(|g| g.id == id) else {
        return;
    };
    let parent = groups.remove(at).parent;
    for g in groups.iter_mut().filter(|g| g.parent == Some(id)) {
        g.parent = parent;
    }
    for m in member_groups.iter_mut().filter(|m| **m == Some(id)) {
        *m = parent;
    }
}

/// The group `id` and every group below it.
pub fn subtree(groups: &[Group], id: u32) -> Vec<u32> {
    let mut out = vec![id];
    let mut at = 0;
    while at < out.len() && out.len() <= groups.len() {
        let parent = out[at];
        let children: Vec<u32> = groups
            .iter()
            .filter(|g| g.parent == Some(parent) && !out.contains(&g.id))
            .map(|g| g.id)
            .collect();
        out.extend(children);
        at += 1;
    }
    out
}

/// Moves group `id` (with everything under it) to be a child of `parent`
/// (`None`: top level), last among its siblings. Refused when `parent` is the
/// group itself or inside it.
pub fn reparent(groups: &mut Vec<Group>, id: u32, parent: Option<u32>) -> Result<(), String> {
    if parent.is_some_and(|p| subtree(groups, id).contains(&p)) {
        return Err("A report group cannot go inside itself.".into());
    }
    let Some(at) = groups.iter().position(|g| g.id == id) else {
        return Ok(());
    };
    let mut g = groups.remove(at);
    g.parent = parent;
    groups.push(g);
    Ok(())
}

/// Moves a group one place up (`up`) or down among its siblings; false when
/// it is already first or last.
pub fn shift(groups: &mut [Group], id: u32, up: bool) -> bool {
    let Some(parent) = groups.iter().find(|g| g.id == id).map(|g| g.parent) else {
        return false;
    };
    let sibs: Vec<usize> = groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.parent == parent)
        .map(|(i, _)| i)
        .collect();
    let Some(k) = sibs.iter().position(|&i| groups[i].id == id) else {
        return false;
    };
    let other = if up {
        k.checked_sub(1)
    } else {
        (k + 1 < sibs.len()).then_some(k + 1)
    };
    match other {
        Some(o) => {
            groups.swap(sibs[k], sibs[o]);
            true
        }
        None => false,
    }
}

/// Moves a group one level out (it becomes a sibling of its parent); false
/// at the top level.
pub fn outdent(groups: &mut Vec<Group>, id: u32) -> bool {
    let Some(parent) = groups.iter().find(|g| g.id == id).and_then(|g| g.parent) else {
        return false;
    };
    let grand = groups
        .iter()
        .find(|g| g.id == parent)
        .and_then(|g| g.parent);
    reparent(groups, id, grand).is_ok()
}

/// Moves a group one level in (it becomes the last child of the sibling
/// before it); false when it has no sibling before it.
pub fn indent(groups: &mut Vec<Group>, id: u32) -> bool {
    let Some(parent) = groups.iter().find(|g| g.id == id).map(|g| g.parent) else {
        return false;
    };
    let before = groups
        .iter()
        .take_while(|g| g.id != id)
        .filter(|g| g.parent == parent)
        .last()
        .map(|g| g.id);
    match before {
        Some(b) => reparent(groups, id, Some(b)).is_ok(),
        None => false,
    }
}

/// Copies group `id` and the groups under it (not their reports) as a
/// child of `parent`; returns the id of the copy.
pub fn copy_subtree(groups: &mut Vec<Group>, id: u32, parent: Option<u32>) -> Option<u32> {
    let members = subtree(groups, id);
    let mut made: Vec<(u32, u32)> = vec![];
    for old in members {
        let g = groups.iter().find(|g| g.id == old)?.clone();
        let new_parent = if old == id {
            parent
        } else {
            made.iter()
                .find(|(o, _)| Some(*o) == g.parent)
                .map(|(_, n)| *n)
        };
        let name = if old == id {
            format!("{} (copy)", g.name)
        } else {
            g.name.clone()
        };
        let new = add_group(groups, new_parent, &name);
        made.push((old, new));
    }
    made.first().map(|(_, n)| *n)
}

/// The group's name with its ancestors (`Activity Reports > Weekly`).
pub fn group_path(groups: &[Group], id: Option<u32>) -> String {
    let mut parts = vec![];
    let mut at = id;
    while let Some(g) = at.and_then(|i| groups.iter().find(|g| g.id == i)) {
        if parts.len() > groups.len() {
            break;
        }
        parts.push(g.name.clone());
        at = g.parent;
    }
    parts.reverse();
    parts.join(" > ")
}

/// A file name for a report: its name (else its title) with the characters
/// a file name cannot hold replaced.
pub fn file_stem(def: &Definition) -> String {
    stem_of(if def.name.trim().is_empty() {
        &def.title
    } else {
        &def.name
    })
}

/// One report of a batch: the file name it gets and how to write it.
pub struct BatchItem<'a> {
    pub stem: String,
    pub write: WriteFn<'a>,
}

/// Writes one report to a file in a format.
pub type WriteFn<'a> = Box<dyn Fn(Output, &Path) -> Result<(), String> + 'a>;

/// Writes each report into `folder` as `format`, one file per report named
/// after it (numbered when two names collide). Returns the files written.
pub fn run_batch_items(
    items: Vec<BatchItem>,
    folder: &Path,
    format: Output,
) -> Result<Vec<PathBuf>, String> {
    if folder.as_os_str().is_empty() {
        return Err("Choose a folder for the reports.".into());
    }
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("{} could not be created: {e}", folder.display()))?;
    let mut written: Vec<PathBuf> = vec![];
    for item in items {
        let mut path = folder.join(format!("{}.{}", item.stem, format.extension()));
        let mut n = 2;
        while written.contains(&path) {
            path = folder.join(format!("{} ({n}).{}", item.stem, format.extension()));
            n += 1;
        }
        if let Err(e) = (item.write)(format, &path) {
            return Err(batch_stopped(&item.stem, &e, &written));
        }
        written.push(path);
    }
    Ok(written)
}

/// The message of a batch that stopped at a failed report: which report,
/// why, and the files already written before it.
fn batch_stopped(stem: &str, error: &str, written: &[PathBuf]) -> String {
    let done = if written.is_empty() {
        "No files were written.".to_string()
    } else {
        let names: Vec<String> = written
            .iter()
            .map(|p| {
                p.file_name()
                    .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into())
            })
            .collect();
        format!(
            "{} file(s) were written before it: {}.",
            written.len(),
            names.join(", ")
        )
    };
    format!("The batch stopped at \"{stem}\": {error} {done}")
}

/// Writes each wizard report into `folder` (see [`run_batch_items`]).
#[cfg(test)]
pub fn run_batch(
    defs: &[&Definition],
    folder: &Path,
    format: Output,
    build: impl Fn(&Definition) -> super::doc::Report,
) -> Result<Vec<PathBuf>, String> {
    let build = &build;
    let items = defs
        .iter()
        .map(|def| BatchItem {
            stem: file_stem(def),
            write: Box::new(move |format, path| super::doc::write(&build(def), format, path)),
        })
        .collect();
    run_batch_items(items, folder, format)
}

/// A file name stem from a report name.
pub fn stem_of(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let stem = stem.trim_end_matches(['.', ' ']).to_string();
    // Names Windows reserves for devices, with or without an extension.
    let base = stem.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.ends_with(|c: char| c.is_ascii_digit()));
    if stem.is_empty() {
        "Report".into()
    } else if reserved {
        format!("_{stem}")
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::wizard::Subject;

    fn report(name: &str, group: Option<u32>) -> Definition {
        let mut d = Definition::new(Subject::Activities);
        d.name = name.into();
        d.group = group;
        d
    }

    #[test]
    fn groups_tree_add_delete_and_json() {
        let mut groups = vec![];
        let a = add_group(&mut groups, None, "Activity Reports");
        let b = add_group(&mut groups, Some(a), "Weekly");
        let c = add_group(&mut groups, None, "Resource Reports");
        assert_eq!((a, b, c), (1, 2, 3));
        let mut reports = vec![
            report("Loose", None),
            report("Lookahead", Some(b)),
            report("Usage", Some(c)),
            report("Orphan", Some(99)),
        ];
        assert_eq!(
            outline(&groups, &reports),
            vec![
                (Node::Group(a), 0),
                (Node::Group(b), 1),
                (Node::Report(1), 2),
                (Node::Group(c), 0),
                (Node::Report(2), 1),
                (Node::Report(0), 0),
                (Node::Report(3), 0),
            ]
        );
        assert_eq!(group_path(&groups, Some(b)), "Activity Reports > Weekly");
        let back = groups_from_json(&groups_to_json(&groups));
        assert_eq!(back, groups);
        // Deleting a group moves its children and reports up a level.
        delete_group(&mut groups, &mut reports, a);
        assert_eq!(groups.iter().find(|g| g.id == b).unwrap().parent, None);
        delete_group(&mut groups, &mut reports, b);
        assert_eq!(reports[1].group, None);
        // The report's group survives the definitions' JSON.
        let saved = crate::reports::wizard::definitions_to_json(&reports);
        let read = crate::reports::wizard::definitions_from_json(&saved);
        assert_eq!(read[2].group, Some(c));
    }

    #[test]
    fn groups_move_indent_outdent_and_copy() {
        let mut g = vec![];
        let a = add_group(&mut g, None, "A");
        let b = add_group(&mut g, None, "B");
        let c = add_group(&mut g, None, "C");
        // Up and down swap with the neighbour among siblings.
        assert!(shift(&mut g, c, true));
        assert_eq!(g.iter().map(|x| x.id).collect::<Vec<_>>(), [a, c, b]);
        assert!(!shift(&mut g, a, true) && !shift(&mut g, b, false));
        // Indent puts a group under the one before it; outdent undoes it.
        assert!(indent(&mut g, b));
        assert_eq!(g.iter().find(|x| x.id == b).unwrap().parent, Some(c));
        assert!(!indent(&mut g, a), "nothing before the first group");
        assert!(outdent(&mut g, b));
        assert_eq!(g.iter().find(|x| x.id == b).unwrap().parent, None);
        assert!(!outdent(&mut g, b), "already at the top");
        // A group cannot go inside itself or what is under it.
        assert!(indent(&mut g, b));
        assert!(reparent(&mut g, c, Some(b)).is_err());
        assert_eq!(subtree(&g, c), vec![c, b]);
        // Copy keeps the shape and renames only the top copy.
        let copy = copy_subtree(&mut g, c, None).unwrap();
        let names: Vec<String> = subtree(&g, copy)
            .iter()
            .map(|id| g.iter().find(|x| x.id == *id).unwrap().name.clone())
            .collect();
        assert_eq!(names, ["C (copy)", "B"]);
    }

    #[test]
    fn file_names_from_report_names_are_safe() {
        for name in [
            "a&b|c^d%e!f(g)<h>",
            "x:y*z?\"q\"",
            "tab\there\r\n",
            "..\\..\\evil/..",
            "  trailing. . ",
        ] {
            let stem = stem_of(name);
            assert!(
                !stem.contains(|c: char| c.is_control() || "<>:\"/\\|?*".contains(c)),
                "{stem}"
            );
            assert!(!stem.ends_with(['.', ' ']), "{stem}");
        }
        assert_eq!(stem_of(""), "Report");
        assert_eq!(stem_of("CON"), "_CON");
        assert_eq!(stem_of("nul.txt"), "_nul.txt");
        assert_eq!(stem_of("com1"), "_com1");
        assert_eq!(stem_of("Console"), "Console");
        assert!(stem_of(&"x".repeat(500)).len() <= 120);
    }

    #[test]
    fn bad_group_json_is_repaired() {
        let text = r#"[{"id":1,"name":"A","parent":2},{"id":2,"name":"B","parent":1},
            {"id":2,"name":"dup"},{"name":"no id"},{"id":3,"name":"C","parent":7}]"#;
        let groups = groups_from_json(text);
        assert_eq!(groups.len(), 3);
        // The loop is broken and the missing parent dropped.
        assert!(groups.iter().any(|g| g.parent.is_none() && g.id != 3));
        assert_eq!(groups[2].parent, None);
        assert_eq!(outline(&groups, &[]).len(), 3);
        assert!(groups_from_json("not json").is_empty());
    }

    #[test]
    fn batch_writes_one_file_per_report() {
        let dir = crate::reports::test_support::temp("batch");
        let p = crate::reports::test_support::scheduled_starter();
        let mut a = report("Weekly: look/ahead", None);
        a.title = "Weekly".into();
        let b = report("Weekly: look/ahead", None);
        let c = report("", None);
        let ix = crate::grid::columns::Index::new((false, 0.));
        let build = |d: &Definition| {
            crate::reports::wizard::build(
                d,
                &p,
                &crate::reports::wizard::Context {
                    ix: &ix,
                    users: &[],
                },
            )
        };
        let files = run_batch(&[&a, &b, &c], &dir, Output::Pdf, build).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "Weekly_ look_ahead.pdf",
                "Weekly_ look_ahead (2).pdf",
                "Activities Report.pdf"
            ]
        );
        for f in &files {
            crate::reports::pdf::check(&std::fs::read(f).unwrap()).unwrap();
        }
        let files = run_batch(&[&a], &dir.join("csv"), Output::Csv, build).unwrap();
        assert!(std::fs::read_to_string(&files[0])
            .unwrap()
            .contains("RB-1000"));
        assert!(run_batch(&[&a], Path::new(""), Output::Csv, build).is_err());
    }

    #[test]
    fn a_batch_that_stops_says_which_files_were_written() {
        let dir = std::env::temp_dir().join(format!("openxer-batch-stop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ok = |_: Output, p: &Path| std::fs::write(p, b"x").map_err(|e| e.to_string());
        let items = vec![
            BatchItem {
                stem: "First".into(),
                write: Box::new(ok),
            },
            BatchItem {
                stem: "Second".into(),
                write: Box::new(|_: Output, _: &Path| Err("disk full".to_string())),
            },
            BatchItem {
                stem: "Third".into(),
                write: Box::new(ok),
            },
        ];
        let e = run_batch_items(items, &dir, Output::Pdf).unwrap_err();
        assert!(e.contains("\"Second\""), "{e}");
        assert!(e.contains("disk full"), "{e}");
        assert!(e.contains("First.pdf"), "{e}");
        assert!(!e.contains("Third"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

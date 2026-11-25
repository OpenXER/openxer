//! Notebook topics of an activity (`TASKMEMO`), a WBS element or the project
//! (`WBSMEMO`; the project's notebook is the one on its own WBS node), by
//! row id: list them, add a topic from the Notebook Topics dictionary
//! (`MEMOTYPE`), change a topic's text and delete a topic. The desktop's
//! Notebook tabs call these; the command line names topics through
//! `tables::set_notebook` and `tables::set_wbs_notebook`.
use super::records::{self, num};
use crate::Document;

type R<T> = Result<T, String>;

/// Whose notebook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Activity(i64),
    Wbs(i64),
    /// The project, through its own WBS node.
    Project(i64),
}

struct Columns {
    table: &'static str,
    key: &'static str,
    owner: &'static str,
    text: &'static str,
    /// The `MEMOTYPE` flag that offers a topic here.
    flag: &'static str,
}

impl Owner {
    fn columns(self) -> Columns {
        match self {
            Owner::Activity(_) => Columns {
                table: "TASKMEMO",
                key: "memo_id",
                owner: "task_id",
                text: "task_memo",
                flag: "task_flag",
            },
            Owner::Wbs(_) | Owner::Project(_) => Columns {
                table: "WBSMEMO",
                key: "wbs_memo_id",
                owner: "wbs_id",
                text: "wbs_memo",
                flag: if matches!(self, Owner::Project(_)) {
                    "proj_flag"
                } else {
                    "wbs_flag"
                },
            },
        }
    }
    fn id(self) -> i64 {
        match self {
            Owner::Activity(id) | Owner::Wbs(id) | Owner::Project(id) => id,
        }
    }
}

/// One notebook topic of an owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memo {
    /// The row's id (`memo_id` or `wbs_memo_id`).
    pub id: i64,
    pub topic_id: i64,
    pub topic: String,
    /// The text as stored (HTML).
    pub text: String,
}

/// The notebook topics of `owner` in project `pid`, in file order.
pub fn memos(doc: &Document, pid: i64, owner: Owner) -> Vec<Memo> {
    let c = owner.columns();
    let Some(t) = doc.table(c.table) else {
        return vec![];
    };
    let has_project = t.field("proj_id").is_some();
    t.rows
        .iter()
        .filter(|r| {
            num(t.value(r, c.owner)) == owner.id()
                && (!has_project || num(t.value(r, "proj_id")) == pid)
        })
        .map(|r| {
            let topic_id = num(t.value(r, "memo_type_id"));
            let name = records::lookup(doc, "MEMOTYPE", "memo_type_id", topic_id, "memo_type");
            Memo {
                id: num(t.value(r, c.key)),
                topic_id,
                topic: if name.is_empty() {
                    topic_id.to_string()
                } else {
                    name
                },
                text: t.value(r, c.text).to_string(),
            }
        })
        .collect()
}

/// The topics of the Notebook Topics dictionary offered to `owner` (its
/// flag is not `N`), in dictionary order: (id, name).
pub fn topics(doc: &Document, owner: Owner) -> Vec<(i64, String)> {
    let flag = owner.columns().flag;
    let Some(t) = doc.table("MEMOTYPE") else {
        return vec![];
    };
    let mut topics: Vec<(i64, i64, String)> = t
        .rows
        .iter()
        .filter(|r| t.value(r, flag) != "N")
        .map(|r| {
            (
                num(t.value(r, "seq_num")),
                num(t.value(r, "memo_type_id")),
                t.value(r, "memo_type").to_string(),
            )
        })
        .collect();
    topics.sort();
    topics.into_iter().map(|(_, id, name)| (id, name)).collect()
}

/// The topics offered to `owner` that it does not have yet.
pub fn unused_topics(doc: &Document, pid: i64, owner: Owner) -> Vec<(i64, String)> {
    let used: Vec<i64> = memos(doc, pid, owner).iter().map(|m| m.topic_id).collect();
    topics(doc, owner)
        .into_iter()
        .filter(|(id, _)| !used.contains(id))
        .collect()
}

/// Give `owner` notebook topic `topic_id` with no text yet. Returns the new
/// row's id. A topic it already has is refused.
pub fn add(doc: &mut Document, pid: i64, owner: Owner, topic_id: i64) -> R<i64> {
    if memos(doc, pid, owner)
        .iter()
        .any(|m| m.topic_id == topic_id)
    {
        return Err("This notebook topic is already there.".into());
    }
    let c = owner.columns();
    let id = records::next_id(doc, c.table, c.key);
    records::add_row(
        doc,
        c.table,
        &[
            (c.key, id.to_string()),
            ("proj_id", pid.to_string()),
            (c.owner, owner.id().to_string()),
            ("memo_type_id", topic_id.to_string()),
            (c.text, records::text_to_memo("")),
        ],
    );
    Ok(id)
}

fn row(doc: &Document, owner: Owner, id: i64) -> R<usize> {
    let c = owner.columns();
    doc.table(c.table)
        .and_then(|t| {
            t.rows.iter().position(|r| {
                num(t.value(r, c.key)) == id && num(t.value(r, c.owner)) == owner.id()
            })
        })
        .ok_or_else(|| "That notebook topic is gone.".to_string())
}

/// Set the text (HTML as stored) of notebook row `id` of `owner`. Returns
/// whether it changed.
pub fn set_text(doc: &mut Document, owner: Owner, id: i64, html: &str) -> R<bool> {
    let c = owner.columns();
    let r = row(doc, owner, id)?;
    if records::get(doc, c.table, r, c.text) == html {
        return Ok(false);
    }
    records::set(doc, c.table, r, c.text, html.to_string());
    Ok(true)
}

/// Delete notebook row `id` of `owner`.
pub fn delete(doc: &mut Document, owner: Owner, id: i64) -> R<()> {
    let c = owner.columns();
    let r = row(doc, owner, id)?;
    records::remove_row(doc, c.table, r);
    Ok(())
}

//! Workspace structures that span projects: the EPS (the folder tree that
//! files every workspace project) and project portfolios. XER files carry
//! neither (a project node only points at an EPS ID), so they live in the
//! workspace database, as small tables kept in one preference row and edited
//! with the same tree rules as the project dictionaries. The desktop app and
//! `openxer-cli` both read and write them through this module.
//!
//! Portfolios have no screen any more (removed 2026-11-07); rows a workspace
//! already holds are kept as stored.
pub mod new_project;
pub mod numbering;
pub mod rollup;
pub mod sample;
pub mod tree;

use crate::edit::records::{self, num};
use crate::{Document, Table};
use std::collections::BTreeMap;
use tree::{Result, TreeSpec};

/// Preference row holding the workspace tables.
pub const KEY: &str = "enterprise_workspace_v1";
/// EPS ID and name of the folder a new workspace starts with.
pub const EPS_ID: &str = "WORKSPACE";
pub const EPS_NAME: &str = "Workspace";

pub const EPS: TreeSpec = TreeSpec {
    table: "EPS",
    id: "eps_id",
    parent: "parent_eps_id",
    seq: "seq_num",
    name: "eps_name",
    code: Some("eps_short_name"),
    owner: None,
    max_depth: 10,
    max_len: 100,
    noun: "EPS node",
    new_name: "New EPS Node",
    code_prefix: "EPS",
    add_child: true,
};

const EPS_FIELDS: [&str; 6] = [
    "eps_id",
    "parent_eps_id",
    "seq_num",
    "eps_short_name",
    "eps_name",
    "obs_name",
];
const ASSIGN_FIELDS: [&str; 2] = ["project_id", "eps_id"];
const PORTFOLIO_FIELDS: [&str; 2] = ["pfolio_id", "pfolio_name"];
const MEMBER_FIELDS: [&str; 2] = ["pfolio_id", "project_id"];

/// The EPS, project placements and portfolios of a workspace. Projects are
/// named by their workspace row id.
#[derive(Clone, Debug)]
pub struct Workspace {
    pub doc: Document,
}

fn empty_table(name: &str, fields: &[&str]) -> Table {
    Table {
        name: name.into(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        rows: vec![],
    }
}

impl Default for Workspace {
    fn default() -> Self {
        let mut doc = Document {
            header: String::new(),
            tables: vec![
                empty_table("EPS", &EPS_FIELDS),
                empty_table("EPSPROJ", &ASSIGN_FIELDS),
                empty_table("PFOLIO", &PORTFOLIO_FIELDS),
                empty_table("PFOLIOPROJ", &MEMBER_FIELDS),
            ],
            windows_encoding: false,
            warnings: vec![],
            lone_quotes: Default::default(),
            id_floor: Default::default(),
        };
        doc.table_mut("EPS").add(&[
            ("eps_id", "1".into()),
            ("seq_num", "10".into()),
            ("eps_short_name", EPS_ID.to_string()),
            ("eps_name", EPS_NAME.to_string()),
        ]);
        Self { doc }
    }
}

impl Workspace {
    /// Text for the preference row.
    pub fn to_text(&self) -> String {
        let tables: Vec<serde_json::Value> = self
            .doc
            .tables
            .iter()
            .map(|t| serde_json::json!({"name": t.name, "fields": t.fields, "rows": t.rows}))
            .collect();
        serde_json::json!({ "tables": tables }).to_string()
    }

    /// Read the preference row; anything unreadable gives the default.
    pub fn from_text(text: &str) -> Self {
        let mut ws = Self::default();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return ws;
        };
        let Some(tables) = value["tables"].as_array() else {
            return ws;
        };
        for t in tables {
            let (Some(name), Some(fields), Some(rows)) = (
                t["name"].as_str(),
                t["fields"].as_array(),
                t["rows"].as_array(),
            ) else {
                continue;
            };
            // Table names match as XER table names do, in any case.
            let Some(slot) = ws
                .doc
                .tables
                .iter_mut()
                .find(|x| x.name.eq_ignore_ascii_case(name))
            else {
                continue;
            };
            slot.fields = fields
                .iter()
                .filter_map(|f| f.as_str().map(String::from))
                .collect();
            slot.rows = rows
                .iter()
                .filter_map(|r| {
                    r.as_array().map(|cells| {
                        cells
                            .iter()
                            .map(|c| c.as_str().unwrap_or("").to_string())
                            .collect()
                    })
                })
                .collect();
        }
        if tree::nodes(&ws.doc, &EPS, None).is_empty() {
            let fresh = Self::default();
            *ws.doc.table_mut("EPS") = fresh.doc.table("EPS").unwrap().clone();
        }
        ws
    }

    /// EPS nodes in display order.
    pub fn eps(&self) -> Vec<tree::Node> {
        tree::nodes(&self.doc, &EPS, None)
    }

    pub fn eps_value(&self, id: i64, column: &str) -> String {
        tree::value(&self.doc, &EPS, id, column)
    }

    /// The node whose EPS ID is `code` (letter case ignored).
    pub fn find(&self, code: &str) -> Option<i64> {
        let code = code.trim();
        self.eps().iter().map(|n| n.id).find(|id| {
            self.eps_value(*id, "eps_short_name")
                .eq_ignore_ascii_case(code)
        })
    }

    /// Names of the node and its parents, top first.
    pub fn path(&self, id: i64) -> Vec<String> {
        let nodes = self.eps();
        let mut out = vec![];
        let mut at = Some(id);
        while let Some(n) = at.and_then(|i| nodes.iter().find(|n| n.id == i)) {
            out.push(self.eps_value(n.id, "eps_name"));
            at = n.parent;
            if out.len() > EPS.max_depth {
                break;
            }
        }
        out.reverse();
        out
    }

    /// The node and the nodes above it, the node first.
    pub fn lineage(&self, id: i64) -> Vec<i64> {
        let nodes = self.eps();
        let mut out = vec![];
        let mut at = Some(id);
        while let Some(n) = at.and_then(|i| nodes.iter().find(|n| n.id == i)) {
            out.push(n.id);
            at = n.parent;
            if out.len() > EPS.max_depth + 1 {
                break;
            }
        }
        out
    }

    /// The node a project sits under: its own when it still exists, else the
    /// first top-level node.
    pub fn node_of(&self, project: Option<i64>) -> i64 {
        let nodes = self.eps();
        let own = project.and_then(|p| {
            let t = self.doc.table("EPSPROJ")?;
            t.rows
                .iter()
                .find(|r| num(t.value(r, "project_id")) == p)
                .map(|r| num(t.value(r, "eps_id")))
        });
        own.filter(|id| nodes.iter().any(|n| n.id == *id))
            .or_else(|| nodes.first().map(|n| n.id))
            .unwrap_or(0)
    }

    /// Put a workspace project under a node; false when it is already there.
    pub fn place(&mut self, project: i64, eps: i64) -> Result<bool> {
        if !self.eps().iter().any(|n| n.id == eps) {
            return Err("That EPS node no longer exists.".into());
        }
        if self.node_of(Some(project)) == eps
            && self.doc.table("EPSPROJ").is_some_and(|t| {
                t.rows
                    .iter()
                    .any(|r| num(t.value(r, "project_id")) == project)
            })
        {
            return Ok(false);
        }
        tree::remove_uses(&mut self.doc, "EPSPROJ", "project_id", &[project]);
        records::add_row(
            &mut self.doc,
            "EPSPROJ",
            &[
                ("project_id", project.to_string()),
                ("eps_id", eps.to_string()),
            ],
        );
        Ok(true)
    }

    /// Drop a deleted workspace project's placement and portfolio memberships.
    pub fn forget(&mut self, project: i64) {
        tree::remove_uses(&mut self.doc, "EPSPROJ", "project_id", &[project]);
        tree::remove_uses(&mut self.doc, "PFOLIOPROJ", "project_id", &[project]);
    }

    /// Workspace projects under the node or one below it.
    pub fn projects_under(&self, eps: i64, projects: &[i64]) -> Vec<i64> {
        let ids = tree::subtree(&self.doc, &EPS, None, eps);
        projects
            .iter()
            .copied()
            .filter(|p| ids.contains(&self.node_of(Some(*p))))
            .collect()
    }

    /// What deleting a node takes with it: the node and the nodes under it
    /// (the node first), and the workspace projects filed in any of them.
    pub fn folder_contents(&self, eps: i64, projects: &[i64]) -> (Vec<i64>, Vec<i64>) {
        (
            tree::subtree(&self.doc, &EPS, None, eps),
            self.projects_under(eps, projects),
        )
    }

    /// The node that stays when `eps` goes: its parent, else the first
    /// remaining top-level node. `None` when `eps` is the last top-level
    /// node, which cannot be deleted.
    pub fn heir(&self, eps: i64) -> Option<i64> {
        let nodes = self.eps();
        let gone = tree::subtree(&self.doc, &EPS, None, eps);
        nodes
            .iter()
            .find(|n| n.id == eps)
            .and_then(|n| n.parent)
            .or_else(|| {
                nodes
                    .iter()
                    .find(|n| n.depth == 0 && !gone.contains(&n.id))
                    .map(|n| n.id)
            })
    }

    /// Delete a node with everything under it: the nodes below it and the
    /// projects filed in any of them (as observed in the scheduler users
    /// know). Returns the projects that go; the caller removes their stored
    /// rows and baselines, this only drops their placement and portfolio
    /// memberships. The EPS keeps at least one node.
    pub fn delete_eps(&mut self, eps: i64, projects: &[i64]) -> Result<Vec<i64>> {
        if self.heir(eps).is_none() {
            return Err("The EPS needs at least one node.".into());
        }
        let going = self.projects_under(eps, projects);
        let gone = tree::delete(&mut self.doc, &EPS, None, eps);
        tree::remove_uses(&mut self.doc, "EPSPROJ", "eps_id", &gone);
        for p in &going {
            self.forget(*p);
        }
        Ok(going)
    }

    /// Add a folder as the last child of `parent` (a top-level folder when
    /// `None`), named `name`, with EPS ID `code` (a generated one when
    /// `None`). Returns the new node.
    pub fn add_folder(
        &mut self,
        parent: Option<i64>,
        name: &str,
        code: Option<&str>,
    ) -> Result<i64> {
        if let Some(p) = parent {
            if !self.eps().iter().any(|n| n.id == p) {
                return Err("That EPS node no longer exists.".into());
            }
        }
        let name = tree::check_name(&self.doc, &EPS, None, parent, None, name)?;
        let code = code
            .map(|c| tree::check_code(&self.doc, &EPS, None, None, c))
            .transpose()?;
        let mut work = self.clone();
        let id = tree::add(&mut work.doc, &EPS, None, parent, &[])?;
        tree::rename(&mut work.doc, &EPS, None, id, EPS.name, &name)?;
        if let Some(code) = code {
            tree::rename(&mut work.doc, &EPS, None, id, "eps_short_name", &code)?;
        }
        *self = work;
        Ok(id)
    }

    /// Rename a folder and/or change its EPS ID. Returns false when both
    /// already have those values.
    pub fn rename_folder(
        &mut self,
        id: i64,
        name: Option<&str>,
        code: Option<&str>,
    ) -> Result<bool> {
        if !self.eps().iter().any(|n| n.id == id) {
            return Err("That EPS node no longer exists.".into());
        }
        let mut work = self.clone();
        let mut changed = false;
        if let Some(name) = name {
            changed |= tree::rename(&mut work.doc, &EPS, None, id, EPS.name, name)?;
        }
        if let Some(code) = code {
            changed |= tree::rename(&mut work.doc, &EPS, None, id, "eps_short_name", code)?;
        }
        *self = work;
        Ok(changed)
    }

    /// Move a folder (with what it holds) to the end of `parent`'s children,
    /// or to the top level when `None`, as Cut and Paste do.
    pub fn move_folder(&mut self, id: i64, parent: Option<i64>) -> Result<bool> {
        let nodes = self.eps();
        let Some(node) = nodes.iter().find(|n| n.id == id) else {
            return Err("That EPS node no longer exists.".into());
        };
        if node.parent == parent {
            return Ok(false);
        }
        if parent.is_some_and(|p| !nodes.iter().any(|n| n.id == p)) {
            return Err("That EPS node no longer exists.".into());
        }
        let clip = tree::copy(&self.doc, &EPS, None, id, true)
            .ok_or_else(|| "That EPS node no longer exists.".to_string())?;
        let mut work = self.clone();
        tree::paste(&mut work.doc, &EPS, None, parent, &clip)?;
        *self = work;
        Ok(true)
    }

    /// EPS rows of the Projects view with the projects under each: (node,
    /// depth, projects), parents before children.
    pub fn grouped(&self, projects: &[i64]) -> Vec<(i64, usize, Vec<i64>)> {
        let mut by_node: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for p in projects {
            by_node.entry(self.node_of(Some(*p))).or_default().push(*p);
        }
        self.eps()
            .iter()
            .map(|n| (n.id, n.depth, by_node.remove(&n.id).unwrap_or_default()))
            .collect()
    }
}

#[cfg(test)]
mod tests;

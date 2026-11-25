//! Global Change definitions saved in a workspace: the app keeps them in the
//! `preferences` row `openxer_xer::edit::global_change::PREFERENCE_KEY`, and
//! a `global_change` edit that names one runs that definition.
use crate::edit::Operation;
use crate::storage::{self, invalid};
use crate::{AutomationError, Result};
use openxer_xer::edit::global_change::{self as gc, GlobalChange};
use rusqlite::OptionalExtension;
use std::path::Path;

/// The definitions saved in workspace `input`; none for an XER input or a
/// workspace without any.
pub fn saved(input: &Path) -> Result<Vec<GlobalChange>> {
    if !storage::is_database(input)? {
        return Ok(vec![]);
    }
    let c = storage::read_database(input)?;
    let has_table = c
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='preferences'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(invalid)?
        .is_some();
    if !has_table {
        return Ok(vec![]);
    }
    let text: Option<String> = c
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [gc::PREFERENCE_KEY],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    match text {
        None => Ok(vec![]),
        Some(text) => gc::from_json(&text).map_err(|e| {
            AutomationError::new(
                "invalid_source",
                format!("The workspace's saved global changes cannot be read: {e}"),
            )
        }),
    }
}

/// `operations` with every `global_change` that names a saved definition
/// (and gives no rule) carrying that definition as its rule. A name that is
/// not saved is refused with the saved names.
pub fn resolve(input: &Path, operations: &[Operation]) -> Result<Vec<Operation>> {
    let named = |op: &Operation| match op {
        Operation::GlobalChange { name, rule: None } => name.clone(),
        _ => None,
    };
    if !operations.iter().any(|op| named(op).is_some()) {
        return Ok(operations.to_vec());
    }
    let list = saved(input)?;
    let mut out = vec![];
    for op in operations {
        let Some(name) = named(op) else {
            out.push(op.clone());
            continue;
        };
        let wanted = name.trim();
        let Some(g) = list.iter().find(|g| g.name.eq_ignore_ascii_case(wanted)) else {
            let names: Vec<&str> = list.iter().map(|g| g.name.as_str()).collect();
            let message = if names.is_empty() {
                format!(
                    "No saved global change named {name:?}: the input has none (they are \
                     saved in an OpenXER workspace); give rule instead"
                )
            } else {
                format!(
                    "No saved global change named {name:?}; saved: {}",
                    names.join(", ")
                )
            };
            return Err(AutomationError::new("invalid_arguments", message));
        };
        let rule = serde_json::to_value(g)
            .map_err(|e| AutomationError::new("invalid_source", e.to_string()))?;
        out.push(Operation::GlobalChange {
            name: Some(g.name.clone()),
            rule: Some(rule),
        });
    }
    Ok(out)
}

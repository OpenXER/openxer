//! The internal project number (`PROJECT.proj_id`). Relationships between
//! projects name the other project by this number (`TASKPRED.pred_proj_id`),
//! so two projects of one workspace must never share it. Every route that
//! makes a project in the workspace (File > New, `eps create-project`, a
//! pasted copy, a baseline restored as a project) gives it a number no
//! workspace project or baseline uses ([`next`]), and [`renumber`] moves a
//! project to another number with every row that carries it.
use crate::{Document, ProjectData};

fn num(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

/// The number after the highest one in `taken` (1 for an empty workspace).
pub fn next(taken: impl IntoIterator<Item = i64>) -> i64 {
    taken.into_iter().max().unwrap_or(0).max(0) + 1
}

/// Columns that name a project by its number in every table that has them.
const PROJECT_COLUMNS: [&str; 2] = ["proj_id", "pred_proj_id"];

/// PROJECT columns that may name the project itself: the project a baseline
/// belongs to, and the project baseline (the project's own number means the
/// current project).
const SELF_COLUMNS: [&str; 2] = ["orig_proj_id", "sum_base_proj_id"];

/// Give project `from` of `doc` the number `to`: every `proj_id` and
/// `pred_proj_id` cell holding `from` (its own rows, and relationships of
/// other projects in the document into it), the PROJECT cells that name it
/// (`orig_proj_id`, `sum_base_proj_id`) and the project UDF values that
/// point at it (`UDFVALUE.fk_id` of a PROJECT field). Nothing else changes.
/// `to` must be a number `doc` does not use.
pub fn renumber(doc: &mut Document, from: i64, to: i64) {
    if from == to {
        return;
    }
    let to_text = to.to_string();
    let project_udfs: Vec<String> = doc
        .table("UDFTYPE")
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| {
                    t.value(r, "table_name")
                        .trim()
                        .eq_ignore_ascii_case("PROJECT")
                })
                .map(|r| t.value(r, "udf_type_id").trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    for t in &mut doc.tables {
        let mut columns: Vec<usize> = PROJECT_COLUMNS.iter().filter_map(|c| t.field(c)).collect();
        if t.name.eq_ignore_ascii_case("PROJECT") {
            columns.extend(SELF_COLUMNS.iter().filter_map(|c| t.field(c)));
        }
        let udf = t
            .name
            .eq_ignore_ascii_case("UDFVALUE")
            .then(|| (t.field("fk_id"), t.field("udf_type_id")));
        for row in &mut t.rows {
            if let Some((Some(fk), Some(kind))) = udf {
                let of_project = row
                    .get(kind)
                    .is_some_and(|k| project_udfs.iter().any(|p| p == k.trim()));
                if of_project && row.get(fk).and_then(|v| num(v)) == Some(from) {
                    row[fk] = to_text.clone();
                }
            }
            for &c in &columns {
                if row.get(c).and_then(|v| num(v)) == Some(from) {
                    row[c] = to_text.clone();
                }
            }
        }
    }
}

/// Every project number `doc` holds (its PROJECT rows).
pub fn in_document(doc: &Document) -> Vec<i64> {
    doc.projects().into_iter().map(|(id, _)| id).collect()
}

impl ProjectData {
    /// Give the project internal number `to` ([`renumber`]) and build it
    /// again from the result, as loading it would. Its activities, dates,
    /// relationships and settings are unchanged. On error the project is
    /// left as it was.
    pub fn renumber_project(&mut self, to: i64) -> Result<(), String> {
        let from = self.model.project.proj_id;
        if from == to {
            return Ok(());
        }
        if in_document(&self.document).contains(&to) {
            return Err(format!("Project number {to} is already in this file"));
        }
        let mut doc = self.updated_document();
        renumber(&mut doc, from, to);
        let mut q = ProjectData::from_document(doc, to, self.source_name.clone())?;
        q.model.options.do_level = self.model.options.do_level;
        q.warnings = self.warnings.clone();
        *self = q;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

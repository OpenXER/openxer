//! Small table helpers shared by the XER merge and export code.
use openxer_xer::{Document, Table};

pub fn num(s: &str) -> i64 {
    let s = s.trim();
    s.parse::<i64>()
        .ok()
        .or_else(|| s.parse::<f64>().ok().map(|v| v as i64))
        .unwrap_or(0)
}

/// Tables whose rows are identified by more than one column. Their first
/// column alone (a UDF type, a financial period, an activity) is shared by
/// many rows, also across projects.
const COMPOSITE_KEYS: &[(&str, &[&str])] = &[
    ("UDFVALUE", &["udf_type_id", "fk_id"]),
    ("TASKFIN", &["fin_dates_id", "task_id"]),
    ("TRSRCFIN", &["fin_dates_id", "taskrsrc_id"]),
    ("TASKACTV", &["task_id", "actv_code_type_id"]),
    ("TASKRISK", &["task_id", "risk_id"]),
    ("PROJPCAT", &["proj_id", "proj_catg_type_id"]),
    ("ASGNMNTACAT", &["taskrsrc_id", "asgnmnt_catg_type_id"]),
];

/// Tables whose first column is the row's own id (project tables; the
/// workspace dictionaries are listed in `openxer_xer::workspace_scope`).
const SINGLE_KEYS: &[(&str, &str)] = &[
    ("PROJECT", "proj_id"),
    ("PROJWBS", "wbs_id"),
    ("TASK", "task_id"),
    ("TASKPRED", "task_pred_id"),
    ("TASKRSRC", "taskrsrc_id"),
    ("TASKMEMO", "memo_id"),
    ("WBSMEMO", "wbs_memo_id"),
    ("TASKPROC", "proc_id"),
    ("PROJCOST", "cost_item_id"),
    ("TASKDOC", "taskdoc_id"),
    ("TASKNOTE", "task_id"),
    ("TASKFDBK", "task_id"),
    ("DOCUMENT", "doc_id"),
    ("PROJRISK", "risk_id"),
    ("RISK", "risk_id"),
    ("PROJISSU", "issue_id"),
    ("ISSUHIST", "issue_id"),
    ("PROJTHRS", "thresh_id"),
    ("BUDGCHNG", "budg_chng_id"),
    ("WBSBUDG", "wbs_budg_id"),
    ("WBSSTEP", "wbs_step_id"),
    ("PROJFUND", "proj_fund_id"),
    ("SCHEDOPTIONS", "schedoptions_id"),
    ("RSRCLEVELLIST", "rsrc_level_list_id"),
];

/// How the rows of a table are told apart, by column names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySpec {
    /// One id column, unique in the table.
    Id(String),
    /// Several columns that together are unique.
    Columns(Vec<String>),
    /// No known key: the whole row.
    Whole,
}

impl KeySpec {
    /// The key of table `name`. `fallback` names the key of a table OpenXER
    /// does not know (its first column), or `None` to compare whole rows.
    pub fn of(name: &str, fallback: Option<&str>) -> Self {
        let upper = name.to_ascii_uppercase();
        if let Some((_, cols)) = COMPOSITE_KEYS.iter().find(|(t, _)| *t == upper) {
            return KeySpec::Columns(cols.iter().map(|c| c.to_string()).collect());
        }
        let single = SINGLE_KEYS
            .iter()
            .find(|(t, _)| *t == upper)
            .map(|(_, c)| *c)
            .or_else(|| openxer_xer::workspace_scope::key_column(&upper));
        match single {
            Some(c) => KeySpec::Id(c.into()),
            None if openxer_xer::workspace_scope::is_workspace_table(&upper) => KeySpec::Whole,
            None => fallback.map_or(KeySpec::Whole, |f| KeySpec::Id(f.into())),
        }
    }
    /// The key reader for rows laid out as `table`.
    pub fn reader(&self, table: &Table) -> KeyReader {
        let columns = match self {
            KeySpec::Id(c) => table.field(c).map(|i| vec![i]),
            KeySpec::Columns(cols) => cols.iter().map(|c| table.field(c)).collect(),
            KeySpec::Whole => None,
        };
        KeyReader {
            columns,
            fields: table
                .fields
                .iter()
                .map(|f| f.to_ascii_lowercase())
                .collect(),
        }
    }
}

/// A [`KeySpec`] with its column positions resolved for one table layout.
/// A key column the table lacks makes the whole row the key.
pub struct KeyReader {
    columns: Option<Vec<usize>>,
    fields: Vec<String>,
}

impl KeyReader {
    /// Whether the whole row is the key.
    pub fn is_whole(&self) -> bool {
        self.columns.is_none()
    }
    /// The key of `row`. A whole-row key lists the nonblank values by
    /// column name, so tables with other column orders compare equal.
    pub fn value(&self, row: &[String]) -> Vec<String> {
        let cell = |i: usize| row.get(i).map_or("", |v| v.trim());
        match &self.columns {
            Some(cols) => cols.iter().map(|&i| cell(i).to_string()).collect(),
            None => {
                let mut pairs: Vec<String> = self
                    .fields
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !cell(*i).is_empty())
                    .map(|(i, f)| format!("{f}={}", cell(i)))
                    .collect();
                pairs.sort();
                pairs
            }
        }
    }
}

/// Largest numeric value of `key` in `table` (or the document's id floor
/// for the table, when higher), plus one.
pub fn next_id(doc: &Document, table: &str, key: &str) -> i64 {
    doc.table(table)
        .map(|t| {
            t.rows
                .iter()
                .map(|r| num(t.value(r, key)))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        .max(doc.id_floor_of(table))
        + 1
}

/// Append a copy of `row` (laid out by `src`) to `dst`. Columns missing from `dst`
/// are added. `overrides` win over copied values. Returns the new row index.
pub fn copy_row(
    dst: &mut Table,
    src: &Table,
    row: &[String],
    overrides: &[(&str, String)],
) -> usize {
    if dst.fields.is_empty() {
        dst.fields = src.fields.clone();
    }
    dst.rows.push(vec![String::new(); dst.fields.len()]);
    let i = dst.rows.len() - 1;
    for (k, field) in src.fields.iter().enumerate() {
        if overrides.iter().any(|(o, _)| o.eq_ignore_ascii_case(field)) {
            continue;
        }
        let value = row.get(k).cloned().unwrap_or_default();
        if !value.is_empty() || dst.field(field).is_some() {
            dst.set(i, field, value);
        }
    }
    for (k, v) in overrides {
        dst.set(i, k, v.clone());
    }
    i
}

/// Overwrite row `i` of `dst` with the values of `row`, except the `keep` columns.
/// Returns true when any value changed.
pub fn update_row(
    dst: &mut Table,
    i: usize,
    src: &Table,
    row: &[String],
    keep: &[&str],
    overrides: &[(&str, String)],
) -> bool {
    let mut changed = false;
    for (k, field) in src.fields.iter().enumerate() {
        if keep.iter().any(|o| o.eq_ignore_ascii_case(field))
            || overrides.iter().any(|(o, _)| o.eq_ignore_ascii_case(field))
        {
            continue;
        }
        let value = row.get(k).cloned().unwrap_or_default();
        if !equivalent(dst.value(&dst.rows[i], field), &value) {
            if value.is_empty() && dst.field(field).is_none() {
                continue;
            }
            dst.set(i, field, value);
            changed = true;
        }
    }
    for (k, v) in overrides {
        if !equivalent(dst.value(&dst.rows[i], k), v) {
            dst.set(i, k, v.clone());
            changed = true;
        }
    }
    changed
}

/// Rows of `table` matching a predicate, as indices.
pub fn rows_where(
    doc: &Document,
    table: &str,
    f: impl Fn(&Table, &[String]) -> bool,
) -> Vec<usize> {
    doc.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| f(t, r))
            .map(|(i, _)| i)
            .collect()
    })
}

/// Remove rows by index from a table (indices may be unsorted).
pub fn remove_rows(doc: &mut Document, table: &str, mut rows: Vec<usize>) {
    if rows.is_empty() || doc.table(table).is_none() {
        return;
    }
    rows.sort_unstable();
    rows.dedup();
    let t = doc.table_mut(table);
    // One pass: removing row by row moved the rest of the table each time.
    let mut doomed = rows.into_iter().peekable();
    let mut i = 0;
    t.rows.retain(|_| {
        let gone = doomed.next_if_eq(&i).is_some();
        i += 1;
        !gone
    });
}

/// A column position resolved once, so a loop over many rows does not
/// search the field names for every value it reads.
#[derive(Clone, Copy)]
pub struct Col(Option<usize>);

impl Col {
    pub fn of(t: &Table, name: &str) -> Self {
        Col(t.field(name))
    }
    /// The cell of `row` in this column, or "" when the table lacks it.
    pub fn get(self, row: &[String]) -> &str {
        self.0.and_then(|i| row.get(i)).map_or("", String::as_str)
    }
}

pub fn same_text(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Values that mean the same in XER: equal text, equal numbers written with a
/// different precision (`8` and `8.000000`), or equal dates in another format.
pub fn equivalent(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (a, b) = (a.trim(), b.trim());
    if a == b {
        return true;
    }
    if let (Ok(x), Ok(y)) = (a.parse::<f64>(), b.parse::<f64>()) {
        return (x - y).abs() < 1e-9;
    }
    match (
        openxer_xer::parse_absolute(a),
        openxer_xer::parse_absolute(b),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

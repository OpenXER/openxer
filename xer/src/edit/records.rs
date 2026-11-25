//! Activity detail records that live in XER tables rather than the scheduling model:
//! steps, notebook topics, activity codes, expenses, documents, feedback, risks,
//! user defined fields and resource assignments. The project document keeps every table, so edits made here
//! survive save, reopen and export without touching the engine model.
use crate::{Document, Table};
use std::sync::Arc;

/// Tables edited outside the scheduling model (Activity Details tabs, WBS,
/// Projects, Resources and project record views, Calendars dialog, enterprise
/// structures, code, UDF, cost account and admin category dictionaries). Undo
/// snapshots these. A
/// snapshot shares every table that did not change with the previous one, so
/// the small dictionary tables cost one comparison per edit and no copy.
#[rustfmt::skip]
pub const DETAIL_TABLES: [&str; 67] = [
    "TASKPROC", "TASKMEMO", "TASKACTV", "PROJCOST", "TASKDOC", "TASKNOTE", "TASKFDBK", "TASKRISK",
    "UDFVALUE", "PROJWBS", "PROJECT", "RSRC", "CALENDAR", "TASKRSRC", "DOCUMENT", "PROJRISK",
    "RISK", "PROJISSU", "ISSUHIST", "PROJTHRS", "THRSPARM",
    // Relationships to other projects live only in the file's rows.
    "TASKPRED",
    // Enterprise dictionaries and the stored period performance.
    "OBS", "ROLES", "ROLERATE", "ROLECATTYPE", "ROLECATVAL", "ROLERCAT", "RSRCCURVDATA", "SHIFT",
    "SHIFTPER", "CURRTYPE", "FUNDSRC", "FINTMPL", "FINDATES", "TASKFIN", "TRSRCFIN",
    // Code, UDF, cost account and admin category dictionaries and the rows
    // that refer to them.
    "ACTVTYPE", "ACTVCODE", "PCATTYPE", "PCATVAL", "PROJPCAT", "RCATTYPE", "RCATVAL", "RSRCRCAT",
    "UDFTYPE", "UDFCODE", "ACCOUNT", "MEMOTYPE", "WBSMEMO", "COSTTYPE", "UMEASURE", "DOCCATG",
    "DOCSTAT", "RISKTYPE", "BASETYPE",
    // Resource prices and roles edited in the Resources view.
    "RSRCRATE", "RSRCROLE",
    // Assignment codes and activity step templates.
    "ASGNMNTCATTYPE", "ASGNMNTCATVAL", "ASGNMNTACAT", "PROCGROUP", "PROCITEM",
    // The budget change log, the spending plan and the WBS milestones.
    "BUDGCHNG", "WBSBUDG", "WBSSTEP",
    // The project's funding (Projects view Funding tab).
    "PROJFUND",
];

/// The project risk register: the reference exports it as `PROJRISK`; a file that
/// carries a `RISK` table instead is read and edited there.
pub fn risk_table(doc: &Document) -> &'static str {
    if doc.table("PROJRISK").is_none() && doc.table("RISK").is_some() {
        "RISK"
    } else {
        "PROJRISK"
    }
}
/// Raw activity columns edited by the details tabs and the activity table
/// that the scheduling model does not own; undo restores them. The edit
/// layer's undo test (`xer/tests/undo_columns.rs`) finds any it misses.
pub const TASK_COLUMNS: [&str; 15] = [
    "duration_type",
    "complete_pct_type",
    "rsrc_id",
    "suspend_date",
    "rev_fdbk_flag",
    "auto_compute_act_flag",
    "priority_type",
    // The activity's own labor and nonlabor units (no assignment).
    "target_work_qty",
    "act_work_qty",
    "remain_work_qty",
    "act_this_per_work_qty",
    "target_equip_qty",
    "act_equip_qty",
    "remain_equip_qty",
    "act_this_per_equip_qty",
];

/// Standard XER column layouts used when a table does not exist yet.
pub fn standard_fields(table: &str) -> &'static [&'static str] {
    match table {
        "TASKPROC" => &[
            "proc_id",
            "task_id",
            "proj_id",
            "seq_num",
            "proc_name",
            "complete_flag",
            "proc_wt",
            "complete_pct",
            "proc_descr",
        ],
        "TASKMEMO" => &["memo_id", "task_id", "memo_type_id", "proj_id", "task_memo"],
        "TASKACTV" => &["task_id", "actv_code_type_id", "actv_code_id", "proj_id"],
        "PROJCOST" => &[
            "cost_item_id",
            "acct_id",
            "pobs_id",
            "cost_type_id",
            "proj_id",
            "task_id",
            "cost_name",
            "po_number",
            "vendor_name",
            "act_cost",
            "cost_per_qty",
            "remain_cost",
            "target_cost",
            "cost_load_type",
            "auto_compute_act_flag",
            "target_qty",
            "qty_name",
            "cost_descr",
        ],
        "TASKDOC" => &[
            "taskdoc_id",
            "doc_id",
            "proj_id",
            "wbs_id",
            "wp_flag",
            "task_id",
        ],
        "TASKNOTE" => &["task_id", "proj_id", "task_notes"],
        "TASKFDBK" => &["task_id", "proj_id", "task_fdbk"],
        "TASKRISK" => &["task_id", "risk_id", "proj_id"],
        "DOCUMENT" => &[
            "doc_id",
            "doc_seq_num",
            "deliv_flag",
            "doc_name",
            "parent_doc_id",
            "proj_id",
            "doc_status_id",
            "doc_catg_id",
            "doc_date",
            "version_name",
            "guid",
            "tmpl_guid",
            "doc_short_name",
            "author_name",
            "private_loc",
            "public_loc",
            "doc_mgmt_type",
            "cr_external_doc_key",
            "doc_content",
        ],
        "PROJRISK" | "RISK" => &[
            "risk_id",
            "risk_code",
            "proj_id",
            "add_date",
            "status_code",
            "risk_name",
            "risk_type_id",
            "rsrc_id",
            "risk_desc",
            "risk_to_type",
            "identified_by_id",
            "response_type",
            "response_text",
            "pre_rsp_prblty",
            "pre_rsp_schd_prblty",
            "pre_rsp_cost_prblty",
            "post_rsp_prblty",
            "post_rsp_schd_prblty",
            "post_rsp_cost_prblty",
            "risk_cause",
            "risk_effect",
            "notes",
            "risk_descr",
        ],
        "PROJISSU" => &[
            "issue_id",
            "obs_id",
            "proj_id",
            "rsrc_id",
            "task_id",
            "thresh_id",
            "thresh_parm_id",
            "track_view_id",
            "wbs_id",
            "add_by_name",
            "issue_name",
            "resolv_date",
            "add_date",
            "priority_type",
            "status_code",
            "base_proj_id",
            "hi_parm_value",
            "issue_value",
            "lo_parm_value",
            "workspace_id",
            "issue_notes",
        ],
        "ISSUHIST" => &["issue_id", "proj_id", "issue_history"],
        "PROJTHRS" => &[
            "thresh_id",
            "proj_id",
            "obs_id",
            "thresh_parm_id",
            "status_code",
            "priority_type",
            "thresh_type",
            "wbs_id",
            "track_view_id",
            "lo_parm_value",
            "hi_parm_value",
            "window_start",
            "window_end",
        ],
        "THRSPARM" => &[
            "thresh_parm_id",
            "seq_num",
            "wbs_flag",
            "task_flag",
            "rsrc_flag",
            "thresh_parm_type",
            "thresh_parm_name",
            "thresh_field_name",
            "thresh_short_name",
        ],
        "RSRC" => &[
            "rsrc_id",
            "parent_rsrc_id",
            "clndr_id",
            "role_id",
            "shift_id",
            "user_id",
            "pobs_id",
            "guid",
            "rsrc_seq_num",
            "email_addr",
            "employee_code",
            "office_phone",
            "other_phone",
            "rsrc_name",
            "rsrc_short_name",
            "rsrc_title_name",
            "def_qty_per_hr",
            "cost_qty_type",
            "ot_factor",
            "active_flag",
            "auto_compute_act_flag",
            "def_cost_qty_link_flag",
            "ot_flag",
            "curr_id",
            "unit_id",
            "rsrc_type",
            "location_id",
            "rsrc_notes",
            "load_tasks_flag",
            "level_flag",
            "last_checksum",
        ],
        "RSRCRATE" => &[
            "rsrc_rate_id",
            "rsrc_id",
            "max_qty_per_hr",
            "cost_per_qty",
            "start_date",
            "shift_period_id",
            "cost_per_qty2",
            "cost_per_qty3",
            "cost_per_qty4",
            "cost_per_qty5",
        ],
        "RSRCROLE" => &["rsrc_id", "role_id", "skill_level"],
        "RSRCRCAT" => &["rsrc_id", "rsrc_catg_type_id", "rsrc_catg_id"],
        "BUDGCHNG" => &[
            "budg_chng_id",
            "proj_id",
            "wbs_id",
            "chng_date",
            "chng_cost",
            "chng_by_name",
            "status_code",
            "chng_descr",
            "chng_num",
        ],
        "WBSBUDG" => &[
            "wbs_budg_id",
            "proj_id",
            "wbs_id",
            "start_date",
            "end_date",
            "spend_cost",
            "benefit_cost",
        ],
        "WBSSTEP" => &[
            "wbs_step_id",
            "proj_id",
            "wbs_id",
            "seq_num",
            "step_name",
            "step_wt",
            "step_descr",
            "complete_flag",
        ],
        "UDFVALUE" => &[
            "udf_type_id",
            "fk_id",
            "proj_id",
            "udf_date",
            "udf_number",
            "udf_text",
            "udf_code_id",
        ],
        "ASGNMNTCATTYPE" | "ASGNMNTCATVAL" | "ASGNMNTACAT" | "PROCGROUP" | "PROCITEM" => {
            crate::standard_fields(table).unwrap_or(&[])
        }
        "PROJPCAT" | "WBSMEMO" => crate::standard_fields(table).unwrap_or(&[]),
        _ => &[],
    }
}

/// Largest magnitude `num` returns. XER ids, counts and hours are far below
/// it; a file value beyond it (say `99999999999999999999`) is clamped so that
/// callers can add to it or scale it without overflowing.
pub const NUM_LIMIT: i64 = 1 << 50;

pub fn num(s: &str) -> i64 {
    let s = s.trim();
    s.parse::<i64>()
        .ok()
        .or_else(|| {
            s.parse::<f64>()
                .ok()
                .filter(|v| !v.is_nan())
                .map(|v| v.clamp(-(NUM_LIMIT as f64), NUM_LIMIT as f64) as i64)
        })
        .unwrap_or(0)
        .clamp(-NUM_LIMIT, NUM_LIMIT)
}
pub fn float(s: &str) -> f64 {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.)
}
/// XER is tab separated with one row per line; free text must not break the framing.
pub fn clean(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ")
}
pub fn format_number(v: f64) -> String {
    if v.fract() == 0. {
        format!("{v:.0}")
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').into()
    }
}

/// Row indices of `table` that belong to the activity. `proj_id` is honored when present.
pub fn rows_for(doc: &Document, table: &str, task_id: i64, pid: i64) -> Vec<usize> {
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    let has_project = t.field("proj_id").is_some();
    t.rows
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            num(t.value(r, "task_id")) == task_id
                && (!has_project || num(t.value(r, "proj_id")) == pid)
        })
        .map(|(i, _)| i)
        .collect()
}
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
/// Get the table, creating it with the standard XER columns when it is missing.
pub fn ensure_table<'a>(doc: &'a mut Document, name: &str) -> &'a mut Table {
    let table = doc.table_mut(name);
    if table.fields.is_empty() {
        table.fields = standard_fields(name)
            .iter()
            .map(|s| s.to_string())
            .collect();
    }
    table
}
pub fn add_row(doc: &mut Document, name: &str, values: &[(&str, String)]) -> usize {
    let table = ensure_table(doc, name);
    table.add(values);
    table.rows.len() - 1
}
pub fn remove_row(doc: &mut Document, name: &str, index: usize) {
    let table = doc.table_mut(name);
    if index < table.rows.len() {
        table.rows.remove(index);
    }
}
pub fn get(doc: &Document, name: &str, index: usize, column: &str) -> String {
    doc.table(name)
        .and_then(|t| t.rows.get(index).map(|r| t.value(r, column).to_string()))
        .unwrap_or_default()
}
pub fn set(doc: &mut Document, name: &str, index: usize, column: &str, value: String) {
    let table = doc.table_mut(name);
    if index < table.rows.len() {
        table.set(index, column, value);
    }
}

fn task_row(doc: &Document, task_id: i64, pid: i64) -> Option<usize> {
    let t = doc.table("TASK")?;
    t.rows
        .iter()
        .position(|r| num(t.value(r, "task_id")) == task_id && num(t.value(r, "proj_id")) == pid)
}
/// A raw activity column the scheduling model does not carry (for example `suspend_date`).
pub fn task_value(doc: &Document, task_id: i64, pid: i64, column: &str) -> String {
    task_row(doc, task_id, pid)
        .map(|i| get(doc, "TASK", i, column))
        .unwrap_or_default()
}
/// Set a raw activity column. New activities get their TASK row here; export fills the rest.
pub fn set_task_value(doc: &mut Document, task_id: i64, pid: i64, column: &str, value: String) {
    let i = task_row(doc, task_id, pid).unwrap_or_else(|| {
        let t = doc.table_mut("TASK");
        t.add(&[
            ("task_id", task_id.to_string()),
            ("proj_id", pid.to_string()),
        ]);
        t.rows.len() - 1
    });
    doc.table_mut("TASK").set(i, column, value);
}

/// Lookup a single value in a dictionary table (e.g. ACTVCODE short_name by actv_code_id).
pub fn lookup(doc: &Document, table: &str, key: &str, id: i64, column: &str) -> String {
    doc.table(table)
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, key)) == id)
                .map(|r| t.value(r, column).to_string())
        })
        .unwrap_or_default()
}

/// Notebook memos are stored as HTML. Show readable text and write simple HTML back.
pub fn memo_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let name = tag
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let closing = tag.starts_with('/');
                if name == "br" || (closing && matches!(name.as_str(), "p" | "div" | "li")) {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    let text = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    text.trim_end_matches('\n').to_string()
}
/// Whether `text` is already HTML (stored as given) rather than plain text
/// (escaped by [`text_to_memo`]): it starts with a tag such as `<p>`,
/// `<html>`, `</b>` or `<!DOCTYPE`. "<5 days float" is plain text.
pub fn looks_like_html(text: &str) -> bool {
    let Some(rest) = text.trim_start().strip_prefix('<') else {
        return false;
    };
    let name = rest.strip_prefix(['/', '!']).unwrap_or(rest);
    let len = name.chars().take_while(char::is_ascii_alphanumeric).count();
    name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name[len..].starts_with(|c: char| c == '>' || c == '/' || c.is_whitespace())
        && name.contains('>')
}

pub fn text_to_memo(text: &str) -> String {
    let body = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\t', " ")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "<br>");
    format!("<html><body>{body}</body></html>")
}

/// Undo state for the detail tables and raw TASK columns. Tables are shared between
/// snapshots until they change, so a long undo stack does not copy large notebooks.
#[derive(Clone, Default, Debug)]
pub struct RecordSnapshot {
    tables: Vec<Option<Arc<Table>>>,
    task_columns: Arc<Vec<(i64, i64, Vec<String>)>>,
}
fn same_table(a: &Table, b: &Table) -> bool {
    a.name == b.name && a.fields == b.fields && a.rows == b.rows
}
impl PartialEq for RecordSnapshot {
    fn eq(&self, other: &Self) -> bool {
        let same = |a: &Option<Arc<Table>>, b: &Option<Arc<Table>>| match (a, b) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || same_table(a, b),
            (None, None) => true,
            _ => false,
        };
        self.task_columns == other.task_columns
            && self.tables.len() == other.tables.len()
            && self
                .tables
                .iter()
                .zip(&other.tables)
                .all(|(a, b)| same(a, b))
    }
}
impl RecordSnapshot {
    /// The captured tables, shared by reference.
    pub fn tables(&self) -> impl Iterator<Item = &Arc<Table>> {
        self.tables.iter().flatten()
    }
    /// Capture every detail table.
    pub fn capture(doc: &Document) -> Self {
        Self::capture_shared(doc, None)
    }
    /// Capture, reusing `previous` tables that have not changed.
    pub fn capture_shared(doc: &Document, previous: Option<&RecordSnapshot>) -> Self {
        let tables = DETAIL_TABLES
            .iter()
            .enumerate()
            .map(|(k, name)| {
                let table = doc.table(name)?;
                match previous
                    .and_then(|p| p.tables.get(k))
                    .and_then(Option::as_ref)
                {
                    Some(old) if same_table(old, table) => Some(old.clone()),
                    _ => Some(Arc::new(table.clone())),
                }
            })
            .collect();
        let task_columns: Vec<(i64, i64, Vec<String>)> = doc
            .table("TASK")
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            num(t.value(r, "task_id")),
                            num(t.value(r, "proj_id")),
                            TASK_COLUMNS
                                .iter()
                                .map(|c| t.value(r, c).to_string())
                                .collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let task_columns = match previous {
            Some(p) if *p.task_columns == task_columns => p.task_columns.clone(),
            _ => Arc::new(task_columns),
        };
        Self {
            tables,
            task_columns,
        }
    }
    pub fn restore(&self, doc: &mut Document) {
        for (name, table) in DETAIL_TABLES.iter().zip(&self.tables) {
            match table {
                // A step usually changes one table: the others are left as
                // they are instead of being copied back.
                Some(table) => {
                    let same = doc
                        .table(name)
                        .is_some_and(|t| t.fields == table.fields && t.rows == table.rows);
                    if !same {
                        *doc.table_mut(name) = Table::clone(table);
                    }
                }
                None => doc.tables.retain(|t| !t.name.eq_ignore_ascii_case(name)),
            }
        }
        let Some(t) = doc
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case("TASK"))
        else {
            return;
        };
        let (id, pid) = (t.field("task_id"), t.field("proj_id"));
        let key = |r: &Vec<String>| {
            let cell = |c: Option<usize>| c.and_then(|c| r.get(c)).map_or(0, |v| num(v));
            (cell(id), cell(pid))
        };
        // TASK rows created for new activities after this snapshot go away with them.
        let kept: std::collections::HashSet<(i64, i64)> = self
            .task_columns
            .iter()
            .map(|(task, project, _)| (*task, *project))
            .collect();
        t.rows.retain(|r| kept.contains(&key(r)));
        // Rows by key, the first one when a key repeats (as a search finds it).
        let mut rows: std::collections::HashMap<(i64, i64), usize> = Default::default();
        for (i, r) in t.rows.iter().enumerate() {
            rows.entry(key(r)).or_insert(i);
        }
        for (task_id, pid, values) in self.task_columns.iter() {
            if let Some(&i) = rows.get(&(*task_id, *pid)) {
                for (column, value) in TASK_COLUMNS.iter().zip(values) {
                    if t.field(column).is_some() || !value.is_empty() {
                        t.set(i, column, value.clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_share_unchanged_tables_and_drop_rows_of_undone_activities() {
        let mut p = crate::starter();
        let pid = p.model.project.proj_id;
        add_row(&mut p.document, "TASKMEMO", &[("memo_id", "1".into())]);
        let first = RecordSnapshot::capture(&p.document);
        add_row(&mut p.document, "TASKNOTE", &[("task_id", "1000".into())]);
        let second = RecordSnapshot::capture_shared(&p.document, Some(&first));
        let memo = DETAIL_TABLES.iter().position(|t| *t == "TASKMEMO").unwrap();
        assert!(Arc::ptr_eq(
            first.tables[memo].as_ref().unwrap(),
            second.tables[memo].as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(&first.task_columns, &second.task_columns));
        let tasks = p.document.table("TASK").unwrap().rows.len();
        set_task_value(&mut p.document, 99999, pid, "suspend_date", "x".into());
        assert_eq!(p.document.table("TASK").unwrap().rows.len(), tasks + 1);
        second.restore(&mut p.document);
        assert_eq!(p.document.table("TASK").unwrap().rows.len(), tasks);
    }
}

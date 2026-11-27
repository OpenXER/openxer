//! XER boundary and presentation metadata. Unknown tables and columns survive export.
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use openxer_engine::features::{LevelPriority, LevelingInputs, PriorityField, ResourceDemand};
use openxer_engine::{
    calendar::{CalendarAdapter, Shifts, WorkCalendar, DAY},
    model::*,
    options::*,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    hash::{Hash, Hasher},
    path::Path,
};
pub mod assignments;
pub mod calendar_data;
pub mod compare;
pub mod critical;
pub use critical::{critical_definition, is_critical, CriticalDefinition};
pub mod delete;
pub mod edit;
pub mod enterprise;
pub mod estimate;
pub mod ev;
pub mod limits;
pub mod manual_units;
pub mod periods;
pub mod sample_project;
pub mod schedule_check;
pub mod scurve;
pub mod spread;
pub mod tree;
pub mod workspace;
pub mod workspace_scope;

#[derive(Clone, Debug)]
pub struct Table {
    pub name: String,
    pub fields: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
impl Table {
    pub fn field(&self, name: &str) -> Option<usize> {
        self.fields
            .iter()
            .position(|s| s.eq_ignore_ascii_case(name))
    }
    pub fn value<'a>(&self, row: &'a [String], name: &str) -> &'a str {
        self.field(name)
            .and_then(|i| row.get(i))
            .map_or("", String::as_str)
    }
    pub fn set(&mut self, row: usize, name: &str, value: String) {
        let col = if let Some(i) = self.field(name) {
            i
        } else {
            self.fields.push(name.into());
            for r in &mut self.rows {
                r.push(String::new());
            }
            self.fields.len() - 1
        };
        self.rows[row].resize(self.fields.len(), String::new());
        self.rows[row][col] = value;
    }
    pub fn add(&mut self, values: &[(&str, String)]) {
        let index = self.rows.len();
        self.rows.push(vec![String::new(); self.fields.len()]);
        for (key, val) in values {
            self.set(index, key, val.clone());
        }
    }
    pub fn field_index(&self) -> FieldIndex {
        let mut map = HashMap::with_capacity(self.fields.len());
        for (i, f) in self.fields.iter().enumerate() {
            map.entry(f.to_ascii_lowercase()).or_insert(i);
        }
        FieldIndex { map }
    }
}

/// Case-insensitive O(1) column lookup for a [`Table`].
#[derive(Clone, Debug, Default)]
pub struct FieldIndex {
    map: HashMap<String, usize>,
}

impl FieldIndex {
    pub fn col(&self, name: &str) -> Option<usize> {
        if name.bytes().all(|b| !b.is_ascii_uppercase()) {
            self.map.get(name).copied()
        } else {
            self.map.get(&name.to_ascii_lowercase()).copied()
        }
    }
    pub fn value<'a>(&self, row: &'a [String], name: &str) -> &'a str {
        self.col(name)
            .and_then(|i| row.get(i))
            .map_or("", String::as_str)
    }
    pub fn get(row: &[String], col: Option<usize>) -> &str {
        col.and_then(|i| row.get(i)).map_or("", String::as_str)
    }
    pub fn ensure(&mut self, table: &mut Table, name: &str) -> usize {
        if let Some(i) = self.col(name) {
            return i;
        }
        let col = table.fields.len();
        table.fields.push(name.into());
        for r in &mut table.rows {
            r.push(String::new());
        }
        self.map.insert(name.to_ascii_lowercase(), col);
        col
    }
    pub fn set(&mut self, table: &mut Table, row: usize, name: &str, value: String) {
        let col = self.ensure(table, name);
        table.rows[row].resize(table.fields.len(), String::new());
        table.rows[row][col] = value;
    }
}
/// Table order of a reference project export. Tables seen in the reference
/// corpus are in their exported order; the remaining task detail tables sit
/// beside their neighbours, and the project record tables (expenses,
/// documents, risks, thresholds, issues) follow the tables they refer to.
/// Dictionaries come before the tables that refer to them and each such level
/// is sorted by name, which places the dictionaries the corpus lacks (cost
/// accounts, categories, project codes, UDF code values). Unknown tables keep
/// their place, and new ones go last.
pub const TABLE_ORDER: &[&str] = &[
    "ACCOUNT",
    "ASGNMNTCATTYPE",
    "BASETYPE",
    "COSTTYPE",
    "CURRTYPE",
    "DOCCATG",
    "DOCSTAT",
    "FINTMPL",
    "FUNDSRC",
    "MEMOTYPE",
    "NONWORK",
    "OBS",
    "PCATTYPE",
    "PROCGROUP",
    "RCATTYPE",
    "RISKTYPE",
    "ROLECATTYPE",
    "RSRCCURVDATA",
    "SHIFT",
    "THRSPARM",
    "UDFTYPE",
    "UMEASURE",
    "ASGNMNTCATVAL",
    "FINDATES",
    "PCATVAL",
    "PROCITEM",
    "RCATVAL",
    "ROLECATVAL",
    "ROLES",
    "SHIFTPER",
    "UDFCODE",
    "ROLERATE",
    "ROLERCAT",
    "PROJECT",
    "CALENDAR",
    "PROJPCAT",
    "PROJFUND",
    "SCHEDOPTIONS",
    "PROJWBS",
    "BUDGCHNG",
    "WBSBUDG",
    "DOCUMENT",
    "PROJTHRS",
    "RSRC",
    "ACTVTYPE",
    "RSRCLEVELLIST",
    "RSRCRATE",
    "RSRCRCAT",
    "RSRCROLE",
    "PROJRISK",
    "TASK",
    "WBSMEMO",
    "WBSSTEP",
    "ACTVCODE",
    "PROJCOST",
    "TASKDOC",
    "TASKFIN",
    "TASKMEMO",
    "TASKNOTE",
    "TASKPRED",
    "TASKPROC",
    "TASKRSRC",
    "ASGNMNTACAT",
    "TRSRCFIN",
    "TASKACTV",
    "TASKFDBK",
    "PROJISSU",
    "ISSUHIST",
    "TASKRISK",
    "UDFVALUE",
];
/// Column lists written by a current reference export, used when OpenXER has to create
/// one of the tables it owns. Existing tables always keep their own columns.
pub fn standard_fields(table: &str) -> Option<&'static [&'static str]> {
    Some(match table.to_ascii_uppercase().as_str() {
        "TASK" => &[
            "task_id",
            "proj_id",
            "wbs_id",
            "clndr_id",
            "phys_complete_pct",
            "rev_fdbk_flag",
            "est_wt",
            "lock_plan_flag",
            "auto_compute_act_flag",
            "complete_pct_type",
            "task_type",
            "duration_type",
            "status_code",
            "task_code",
            "task_name",
            "rsrc_id",
            "total_float_hr_cnt",
            "free_float_hr_cnt",
            "remain_drtn_hr_cnt",
            "act_work_qty",
            "remain_work_qty",
            "target_work_qty",
            "target_drtn_hr_cnt",
            "target_equip_qty",
            "act_equip_qty",
            "remain_equip_qty",
            "cstr_date",
            "act_start_date",
            "act_end_date",
            "late_start_date",
            "late_end_date",
            "expect_end_date",
            "early_start_date",
            "early_end_date",
            "restart_date",
            "reend_date",
            "target_start_date",
            "target_end_date",
            "rem_late_start_date",
            "rem_late_end_date",
            "cstr_type",
            "priority_type",
            "suspend_date",
            "resume_date",
            "float_path",
            "float_path_order",
            "guid",
            "tmpl_guid",
            "cstr_date2",
            "cstr_type2",
            "driving_path_flag",
            "act_this_per_work_qty",
            "act_this_per_equip_qty",
            "external_early_start_date",
            "external_late_end_date",
            "create_date",
            "update_date",
            "create_user",
            "update_user",
            "location_id",
            "crt_path_num",
        ],
        "TASKPRED" => &[
            "task_pred_id",
            "task_id",
            "pred_task_id",
            "proj_id",
            "pred_proj_id",
            "pred_type",
            "lag_hr_cnt",
            "comments",
            "float_path",
            "aref",
            "arls",
        ],
        "TASKRSRC" => &[
            "taskrsrc_id",
            "task_id",
            "proj_id",
            "cost_qty_link_flag",
            "role_id",
            "acct_id",
            "rsrc_id",
            "pobs_id",
            "skill_level",
            "remain_qty",
            "target_qty",
            "remain_qty_per_hr",
            "target_lag_drtn_hr_cnt",
            "target_qty_per_hr",
            "act_ot_qty",
            "act_reg_qty",
            "relag_drtn_hr_cnt",
            "ot_factor",
            "cost_per_qty",
            "target_cost",
            "act_reg_cost",
            "act_ot_cost",
            "remain_cost",
            "act_start_date",
            "act_end_date",
            "restart_date",
            "reend_date",
            "target_start_date",
            "target_end_date",
            "rem_late_start_date",
            "rem_late_end_date",
            "rollup_dates_flag",
            "target_crv",
            "remain_crv",
            "actual_crv",
            "ts_pend_act_end_flag",
            "guid",
            "rate_type",
            "act_this_per_cost",
            "act_this_per_qty",
            "curv_id",
            "rsrc_type",
            "cost_per_qty_source_type",
            "create_user",
            "create_date",
            "has_rsrchours",
            "taskrsrc_sum_id",
        ],
        "SCHEDOPTIONS" => &[
            "schedoptions_id",
            "proj_id",
            "sched_outer_depend_type",
            "sched_open_critical_flag",
            "sched_lag_early_start_flag",
            "sched_retained_logic",
            "sched_setplantoforecast",
            "sched_float_type",
            "sched_calendar_on_relationship_lag",
            "sched_use_expect_end_flag",
            "sched_progress_override",
            "level_float_thrs_cnt",
            "level_outer_assign_flag",
            "level_outer_assign_priority",
            "level_over_alloc_pct",
            "level_within_float_flag",
            "level_keep_sched_date_flag",
            "level_all_rsrc_flag",
            "sched_use_project_end_date_for_float",
            "enable_multiple_longest_path_calc",
            "limit_multiple_longest_path_calc",
            "max_multiple_longest_path",
            "use_total_float_multiple_longest_paths",
            "key_activity_for_multiple_longest_paths",
            "LevelPriorityList",
        ],
        "RSRCLEVELLIST" => &["rsrc_level_list_id", "schedoptions_id", "rsrc_id"],
        "CALENDAR" => &[
            "clndr_id",
            "default_flag",
            "clndr_name",
            "proj_id",
            "base_clndr_id",
            "last_chng_date",
            "clndr_type",
            "day_hr_cnt",
            "week_hr_cnt",
            "month_hr_cnt",
            "year_hr_cnt",
            "rsrc_private",
            "clndr_data",
        ],
        "PROJWBS" => &[
            "wbs_id",
            "proj_id",
            "obs_id",
            "seq_num",
            "est_wt",
            "proj_node_flag",
            "sum_data_flag",
            "status_code",
            "wbs_short_name",
            "wbs_name",
            "phase_id",
            "parent_wbs_id",
            "ev_user_pct",
            "ev_etc_user_value",
            "orig_cost",
            "indep_remain_total_cost",
            "ann_dscnt_rate_pct",
            "dscnt_period_type",
            "indep_remain_work_qty",
            "anticip_start_date",
            "anticip_end_date",
            "ev_compute_type",
            "ev_etc_compute_type",
            "guid",
            "tmpl_guid",
            "plan_open_state",
        ],
        // Dictionaries: the database columns without the audit columns, in
        // database order; code types carry `export_flag`, as recent export
        // files do.
        "ACCOUNT" => &[
            "acct_id",
            "parent_acct_id",
            "acct_seq_num",
            "acct_name",
            "acct_short_name",
            "acct_descr",
        ],
        "ACTVTYPE" => &[
            "actv_code_type_id",
            "actv_short_len",
            "seq_num",
            "actv_code_type",
            "proj_id",
            "wbs_id",
            "actv_code_type_scope",
            "export_flag",
        ],
        "ACTVCODE" => &[
            "actv_code_id",
            "parent_actv_code_id",
            "actv_code_type_id",
            "actv_code_name",
            "short_name",
            "seq_num",
            "color",
            "total_assignments",
        ],
        "BASETYPE" => &["base_type_id", "base_type_seq_num", "base_type"],
        "COSTTYPE" => &["cost_type_id", "seq_num", "cost_type"],
        "DOCCATG" => &["doc_catg_id", "seq_num", "doc_catg_name"],
        "DOCSTAT" => &["doc_status_id", "seq_num", "doc_status_code"],
        "MEMOTYPE" => &[
            "memo_type_id",
            "seq_num",
            "eps_flag",
            "proj_flag",
            "wbs_flag",
            "task_flag",
            "memo_type",
        ],
        "PCATTYPE" => &[
            "proj_catg_type_id",
            "seq_num",
            "proj_catg_short_len",
            "proj_catg_type",
            "export_flag",
        ],
        "PCATVAL" => &[
            "proj_catg_id",
            "proj_catg_type_id",
            "seq_num",
            "proj_catg_short_name",
            "parent_proj_catg_id",
            "proj_catg_name",
        ],
        "PROJPCAT" => &["proj_id", "proj_catg_type_id", "proj_catg_id"],
        "RCATTYPE" => &[
            "rsrc_catg_type_id",
            "seq_num",
            "rsrc_catg_short_len",
            "rsrc_catg_type",
            "export_flag",
        ],
        "RCATVAL" => &[
            "rsrc_catg_id",
            "rsrc_catg_type_id",
            "seq_num",
            "rsrc_catg_short_name",
            "rsrc_catg_name",
            "parent_rsrc_catg_id",
        ],
        "RSRCRCAT" => &["rsrc_id", "rsrc_catg_type_id", "rsrc_catg_id"],
        "RISKTYPE" => &[
            "risk_type_id",
            "seq_num",
            "risk_type",
            "parent_risk_type_id",
        ],
        "UDFTYPE" => &[
            "udf_type_id",
            "table_name",
            "udf_type_name",
            "udf_type_label",
            "logical_data_type",
            "super_flag",
            "indicator_expression",
            "summary_indicator_expression",
            "export_flag",
        ],
        "UDFCODE" => &["udf_code_id", "udf_type_id", "short_name", "udf_code_name"],
        "UMEASURE" => &["unit_id", "seq_num", "unit_abbrev", "unit_name"],
        "WBSMEMO" => &[
            "wbs_memo_id",
            "proj_id",
            "memo_type_id",
            "wbs_id",
            "wbs_memo",
        ],
        // Assignment codes: schema columns; the type ends with `export_flag`
        // in place of `super_flag`, as the other code types in recent export
        // files do.
        "ASGNMNTCATTYPE" => &[
            "asgnmnt_catg_type_id",
            "seq_num",
            "asgnmnt_catg_short_len",
            "asgnmnt_catg_type",
            "export_flag",
        ],
        "ASGNMNTCATVAL" => &[
            "asgnmnt_catg_id",
            "asgnmnt_catg_type_id",
            "seq_num",
            "asgnmnt_catg_short_name",
            "asgnmnt_catg_name",
            "parent_asgnmnt_catg_id",
        ],
        "ASGNMNTACAT" => &[
            "taskrsrc_id",
            "asgnmnt_catg_type_id",
            "asgnmnt_catg_id",
            "proj_id",
        ],
        "PROJFUND" => &["proj_fund_id", "proj_id", "fund_id", "fund_cost", "fund_wt"],
        "PROCGROUP" => &["proc_group_id", "seq_num", "proc_group_name"],
        "PROCITEM" => &[
            "proc_item_id",
            "proc_group_id",
            "seq_num",
            "proc_name",
            "proc_wt",
            "proc_descr",
        ],
        _ => return enterprise::standard_fields(table),
    })
}
fn table_rank(name: &str) -> Option<usize> {
    TABLE_ORDER
        .iter()
        .position(|t| t.eq_ignore_ascii_case(name))
}
/// The text a stored value means. A quote mark inside any value is written as
/// two quote marks, in every table and every column, wherever it sits (a name
/// that ends in an inch mark ends in two quote marks, a value that starts with
/// a quote starts with two). Reading turns each pair back into one quote mark.
/// A lone quote mark, which a hand-made file can hold, is kept as it is.
pub fn unescape_quotes(raw: &str) -> Cow<'_, str> {
    if raw.contains("\"\"") {
        Cow::Owned(raw.replace("\"\"", "\""))
    } else {
        Cow::Borrowed(raw)
    }
}
/// The file form of a value: every quote mark written as two.
pub fn escape_quotes(value: &str) -> Cow<'_, str> {
    if value.contains('"') {
        Cow::Owned(value.replace('"', "\"\""))
    } else {
        Cow::Borrowed(value)
    }
}
fn hash_of(value: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}
/// Append one value so that it cannot break the XER structure, whatever its
/// source (file, spreadsheet, UI or automation). Tabs, line feeds and CR LF
/// pairs become a space; reference memos are HTML, where those are only whitespace.
/// A lone CR is kept only when an ordinary character follows it, because the reference
/// itself exports such memo text and an untouched row must stay byte-identical;
/// before `%`, whitespace or the end of the value it becomes a space, so no
/// value can begin a line that a reader splitting on CR would take as a record.
/// The header keeps its tabs, which separate its fields.
fn push_value(out: &mut String, value: &str, keep_tabs: bool) {
    if !value
        .bytes()
        .any(|b| b == b'\t' || b == b'\r' || b == b'\n')
    {
        out.push_str(value);
        return;
    }
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\t' if keep_tabs => out.push('\t'),
            '\t' | '\n' => out.push(' '),
            '\r' => match chars.peek() {
                Some('\n') => {
                    chars.next();
                    out.push(' ');
                }
                Some(&next) if next != '%' && !next.is_whitespace() => out.push('\r'),
                _ => out.push(' '),
            },
            _ => out.push(c),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Document {
    pub header: String,
    pub tables: Vec<Table>,
    pub windows_encoding: bool,
    /// Problems the reader tolerated, such as a missing `%E` end marker.
    pub warnings: Vec<String>,
    /// Values are held as text (a quote mark is one character) and written
    /// with every quote mark doubled. A value the file held with a lone quote
    /// mark would change when written that way, so its file form is kept here,
    /// keyed by its text, and written back unchanged while the value is.
    pub lone_quotes: BTreeMap<String, String>,
    /// Highest id already taken elsewhere in the workspace, by upper-case
    /// table name: a new row's id is above it (`edit::records::next_id`).
    /// Memory only; set when workspace dictionary rows are hydrated
    /// (`workspace_scope`), so a new row never reuses a workspace id.
    pub id_floor: BTreeMap<String, i64>,
}
/// Nonblank text after `%E`, rows shorter than their field list, and similar
/// tolerated problems are reported once per kind.
struct ParseState {
    doc: Document,
    current: Option<usize>,
    merging: bool,
    short_rows: BTreeMap<String, usize>,
    /// Rows with empty values past the last column, by table.
    long_rows: BTreeMap<String, usize>,
    /// Hashes of quoted values that the file wrote in the doubled form.
    doubled: HashSet<u64>,
}
/// Decode one stored value. A value whose quote marks are not all in pairs
/// would not survive writing in the doubled form; its file form is remembered.
fn decode_value(
    raw: &str,
    lone: &mut BTreeMap<String, String>,
    doubled: &mut HashSet<u64>,
) -> String {
    if !raw.contains('"') {
        return raw.to_string();
    }
    let value = unescape_quotes(raw).into_owned();
    if escape_quotes(&value) == raw {
        doubled.insert(hash_of(&value));
    } else {
        lone.insert(value.clone(), raw.to_string());
    }
    value
}
/// Whether a line starts an XER record: `%T`, `%F`, `%R` or `%E`, then a tab
/// or the end of the line (trailing spaces allowed).
fn starts_record(line: &str) -> bool {
    let line = line.trim_end();
    ["%T", "%F", "%R", "%E"].iter().any(|tag| {
        line.strip_prefix(tag)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('\t'))
    })
}
impl ParseState {
    fn record(&mut self, line_number: usize, line: &str) -> Result<bool, String> {
        let mut parts = line.split('\t');
        let tag = parts.next().unwrap_or("");
        match tag {
            "%T" => {
                let name = parts.next().unwrap_or("").trim();
                if name.is_empty() {
                    return Err(format!("Line {line_number}: %T has no table name"));
                }
                if let Some(i) = self
                    .doc
                    .tables
                    .iter()
                    .position(|t| t.name.eq_ignore_ascii_case(name))
                {
                    self.current = Some(i);
                    self.merging = true;
                } else {
                    self.doc.tables.push(Table {
                        name: name.into(),
                        fields: vec![],
                        rows: vec![],
                    });
                    self.current = Some(self.doc.tables.len() - 1);
                    self.merging = false;
                }
            }
            "%F" => {
                let i = self
                    .current
                    .ok_or_else(|| format!("Line {line_number}: %F comes before any %T table"))?;
                let fields: Vec<String> = parts.map(String::from).collect();
                let table = &mut self.doc.tables[i];
                if self.merging {
                    if table.fields != fields {
                        return Err(format!(
                            "Line {line_number}: table {} appears twice with different columns",
                            table.name
                        ));
                    }
                    self.doc.warnings.push(format!(
                        "Table {} appears more than once; its rows were combined.",
                        table.name
                    ));
                    self.merging = false;
                } else if !table.fields.is_empty() {
                    return Err(format!(
                        "Line {line_number}: table {} has a second %F field list",
                        table.name
                    ));
                } else {
                    table.fields = fields;
                }
            }
            "%R" => {
                let i = self
                    .current
                    .ok_or_else(|| format!("Line {line_number}: %R comes before any %T table"))?;
                let table = &mut self.doc.tables[i];
                if table.fields.is_empty() || self.merging {
                    return Err(format!(
                        "Line {line_number}: %R row in table {} comes before its %F field list",
                        table.name
                    ));
                }
                let mut row: Vec<String> = parts
                    .map(|raw| decode_value(raw, &mut self.doc.lone_quotes, &mut self.doubled))
                    .collect();
                if row.len() > table.fields.len()
                    && row[table.fields.len()..].iter().all(String::is_empty)
                {
                    // Empty values after the last column (stray trailing tabs).
                    *self.long_rows.entry(table.name.clone()).or_default() += 1;
                    row.truncate(table.fields.len());
                }
                if row.len() > table.fields.len() {
                    return Err(format!(
                        "Line {line_number}: table {} row has {} values but only {} columns",
                        table.name,
                        row.len(),
                        table.fields.len()
                    ));
                }
                if row.len() < table.fields.len() {
                    *self.short_rows.entry(table.name.clone()).or_default() += 1;
                    row.resize(table.fields.len(), String::new());
                }
                table.rows.push(row);
            }
            "%E" => return Ok(true),
            _ => {
                let shown: String = tag.chars().take(20).collect();
                return Err(format!(
                    "Line {line_number}: unknown XER record \"{shown}\" (expected %T, %F, %R or %E)"
                ));
            }
        }
        Ok(false)
    }
}
impl Document {
    pub fn read(path: &Path) -> Result<Self, String> {
        let data =
            std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        Self::from_bytes(&data)
    }
    /// Decode raw file bytes. Valid UTF-8 with non-ASCII text is read as UTF-8;
    /// everything else, including plain ASCII, is Windows-1252 as the reference writes it.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let (text, legacy) = match std::str::from_utf8(data) {
            Ok(s) => (s.to_string(), s.is_ascii()),
            Err(_) => (encoding_rs::WINDOWS_1252.decode(data).0.into_owned(), true),
        };
        let mut result = Self::parse(&text)?;
        result.windows_encoding = legacy;
        Ok(result)
    }
    /// Parse XER text. Lines may end in CRLF or LF and a leading byte order mark
    /// is ignored. A line that does not start with `%` continues the previous
    /// row's last value (a line break inside a memo); export writes that break
    /// as a space. Export always writes CRLF.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut pieces = text.split('\n').enumerate();
        let header = pieces
            .next()
            .map(|(_, h)| h.strip_suffix('\r').unwrap_or(h))
            .filter(|h| !h.trim().is_empty())
            .ok_or("Empty XER file")?;
        if !header.starts_with("ERMHDR") {
            return Err("This is not an XER file (ERMHDR header is missing)".into());
        }
        let mut state = ParseState {
            doc: Self {
                header: header.to_string(),
                tables: vec![],
                windows_encoding: false,
                warnings: vec![],
                lone_quotes: BTreeMap::new(),
                id_floor: BTreeMap::new(),
            },
            current: None,
            merging: false,
            short_rows: BTreeMap::new(),
            long_rows: BTreeMap::new(),
            doubled: HashSet::new(),
        };
        let mut pending: Option<(usize, String)> = None;
        // Blank lines met inside a pending row: part of its last value when
        // more of the value follows, otherwise only space between records.
        let mut blanks: Vec<&str> = vec![];
        let mut ended = false;
        let mut trailing = false;
        for (index, piece) in pieces {
            let line_number = index + 1;
            let blank = piece.trim_end_matches('\r').is_empty();
            if ended {
                trailing |= !piece.trim().is_empty();
                continue;
            }
            let in_row = pending.as_ref().is_some_and(|(_, r)| r.starts_with("%R"));
            if in_row && blank {
                blanks.push(piece);
                continue;
            }
            // Only %T, %F, %R and %E followed by a tab or the line's end
            // start a record; inside a row any other line (one starting
            // with "% complete", say) continues its last value.
            let continues =
                !blank && !(piece.starts_with('%') && (!in_row || starts_record(piece)));
            if continues {
                if let Some((_, row)) = pending.as_mut().filter(|_| in_row) {
                    for b in blanks.drain(..) {
                        row.push('\n');
                        row.push_str(b);
                    }
                    row.push('\n');
                    row.push_str(piece);
                    continue;
                }
            }
            blanks.clear();
            if let Some((n, record)) = pending.take() {
                if state.record(n, record.strip_suffix('\r').unwrap_or(&record))? {
                    ended = true;
                    trailing |= !piece.trim().is_empty();
                    continue;
                }
            }
            if continues {
                let shown: String = piece.chars().take(20).collect();
                return Err(format!(
                    "Line {line_number}: unexpected text \"{shown}\" outside an XER record"
                ));
            }
            if !blank {
                pending = Some((line_number, piece.to_string()));
            }
        }
        if let Some((n, record)) = pending.take() {
            ended |= state.record(n, record.strip_suffix('\r').unwrap_or(&record))?;
        }
        let mut doc = state.doc;
        // A text written both ways in one file cannot be told apart by its
        // text; the doubled form wins, as the reference writes it.
        let doubled = state.doubled;
        doc.lone_quotes
            .retain(|k, _| !doubled.contains(&hash_of(k)));
        for (table, count) in state.short_rows {
            doc.warnings.push(format!(
                "Table {table}: {count} row(s) have fewer values than columns; the missing values are blank."
            ));
        }
        for (table, count) in state.long_rows {
            doc.warnings.push(format!(
                "Table {table}: {count} row(s) have empty values past the last column; they were dropped."
            ));
        }
        if !ended {
            doc.warnings
                .push("The file has no %E end marker, so it may be truncated.".into());
        }
        if trailing {
            doc.warnings
                .push("Text after the %E end marker was ignored.".into());
        }
        Ok(doc)
    }
    /// The id floor of `table` (see `id_floor`), 0 when none is set.
    pub fn id_floor_of(&self, table: &str) -> i64 {
        self.id_floor
            .get(&table.to_ascii_uppercase())
            .copied()
            .unwrap_or(0)
    }
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }
    /// Take over the lone-quote values of a document whose rows are being
    /// copied into this one, so they are written as they were read.
    pub fn keep_lone_quotes_of(&mut self, other: &Document) {
        for (value, raw) in &other.lone_quotes {
            self.lone_quotes
                .entry(value.clone())
                .or_insert_with(|| raw.clone());
        }
    }
    /// The named table, created when missing. A new table is placed where a reference
    /// export puts it and starts with the standard columns for tables OpenXER owns.
    pub fn table_mut(&mut self, name: &str) -> &mut Table {
        let idx = self
            .tables
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                let table = Table {
                    name: name.into(),
                    fields: standard_fields(name)
                        .unwrap_or(&[])
                        .iter()
                        .map(|f| f.to_string())
                        .collect(),
                    rows: vec![],
                };
                let at = table_rank(name)
                    .and_then(|rank| {
                        self.tables
                            .iter()
                            .position(|t| table_rank(&t.name).is_some_and(|r| r > rank))
                    })
                    .unwrap_or(self.tables.len());
                self.tables.insert(at, table);
                at
            });
        &mut self.tables[idx]
    }
    /// A value as the file stores it: every quote mark doubled, except a value
    /// the file held with a lone quote mark, which is written as it was read.
    fn file_form<'a>(&'a self, value: &'a str) -> Cow<'a, str> {
        if !value.contains('"') {
            return Cow::Borrowed(value);
        }
        match self.lone_quotes.get(value) {
            Some(raw) => Cow::Borrowed(raw),
            None => escape_quotes(value),
        }
    }
    /// The XER text, always CRLF. Values are written through `push_value`, so
    /// no value can split a row or start a line of its own.
    pub fn text(&self) -> String {
        let size = self
            .tables
            .iter()
            .flat_map(|t| &t.rows)
            .map(|r| r.iter().map(|v| v.len() + 1).sum::<usize>() + 4)
            .sum::<usize>();
        let mut s = String::with_capacity(size + self.header.len() + 4096);
        push_value(&mut s, &self.header, true);
        s.push_str("\r\n");
        let push_list = |s: &mut String, values: &[String], rows: bool| {
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    s.push('\t');
                }
                if rows {
                    push_value(s, &self.file_form(v), false);
                } else {
                    push_value(s, v, false);
                }
            }
        };
        for t in &self.tables {
            s.push_str("%T\t");
            push_value(&mut s, &t.name, false);
            s.push_str("\r\n");
            if !t.fields.is_empty() {
                s.push_str("%F\t");
                push_list(&mut s, &t.fields, false);
                s.push_str("\r\n");
            }
            for r in &t.rows {
                s.push_str("%R\t");
                push_list(&mut s, r, true);
                s.push_str("\r\n");
            }
        }
        s.push_str("%E\r\n");
        s
    }
    /// Where the first character that Windows-1252 cannot represent is.
    fn first_unencodable(&self) -> Option<String> {
        let bad = |text: &str| {
            text.chars()
                .find(|c| !c.is_ascii() && encoding_rs::WINDOWS_1252.encode(&c.to_string()).2)
        };
        let describe = |c: char, place: String| {
            format!(
                "\"{c}\" (U+{:04X}) in {place} cannot be written in this XER's Windows-1252 encoding. Replace it before exporting.",
                c as u32
            )
        };
        if let Some(c) = bad(&self.header) {
            return Some(describe(c, "the file header".into()));
        }
        for t in &self.tables {
            if let Some(c) = t.fields.iter().find_map(|f| bad(f)) {
                return Some(describe(c, format!("the {} column names", t.name)));
            }
            for (r, row) in t.rows.iter().enumerate() {
                for (i, value) in row.iter().enumerate() {
                    if let Some(c) = bad(value) {
                        let column = t.fields.get(i).map_or("?", String::as_str);
                        return Some(describe(c, format!("{}.{column} (row {})", t.name, r + 1)));
                    }
                }
            }
        }
        None
    }
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let bytes = self.to_bytes()?;
        write_file_atomic(path, &bytes)
    }
    /// Encoded file contents, in the encoding the document was read with.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let text = self.text();
        if self.windows_encoding {
            let (data, _, errors) = encoding_rs::WINDOWS_1252.encode(&text);
            if errors {
                return Err(self.first_unencodable().unwrap_or_else(|| {
                    "A character cannot be represented in this XER's Windows-1252 encoding".into()
                }));
            }
            Ok(data.into_owned())
        } else {
            Ok(text.into_bytes())
        }
    }
    pub fn projects(&self) -> Vec<(i64, String)> {
        self.table("PROJECT").map_or(vec![], |t| {
            t.rows
                .iter()
                .filter_map(|r| {
                    Some((
                        t.value(r, "proj_id").parse().ok()?,
                        t.value(r, "proj_short_name").into(),
                    ))
                })
                .collect()
        })
    }
}
/// Write `bytes` to `path` through a temporary file in the same folder that
/// is then renamed over `path`. A write that fails part way (a full disk, a
/// lost network drive) leaves the file that was there, not a cut-short one.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} does not name a file", path.display()))?;
    let mut temporary = std::ffi::OsString::from(".");
    temporary.push(name);
    temporary.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = path.with_file_name(temporary);
    let written = std::fs::File::create(&temporary)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, path));
    written.map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        e.to_string()
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wbs {
    pub id: i64,
    pub parent: Option<i64>,
    pub code: String,
    pub name: String,
    pub sequence: i64,
}
#[derive(Clone, Debug, Default)]
pub struct ActivityDetails {
    pub name: String,
    pub wbs: i64,
    pub original_duration: i64,
    pub resource_driven_duration: bool,
    pub percent: f64,
    /// The GUID and create date of an activity made since the project was
    /// read, minted once when it was made (`ProjectData::mint`), so every
    /// save and export writes the same values until its TASK row carries
    /// them. `None` for activities read from a file: their row holds them.
    pub minted: Option<Minted>,
}
/// Equal when every value a user sees and edits is equal. `minted` is left
/// out: it is the identity of a new row, not its content, and a reread
/// project carries it in its TASK row instead.
impl PartialEq for ActivityDetails {
    fn eq(&self, o: &Self) -> bool {
        self.name == o.name
            && self.wbs == o.wbs
            && self.original_duration == o.original_duration
            && self.resource_driven_duration == o.resource_driven_duration
            && self.percent == o.percent
    }
}
/// A GUID and create date (`YYYY-MM-DD HH:MM`) given to a new record once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Minted {
    pub guid: String,
    pub date: String,
}
/// A relationship between activities of two projects: a `TASKPRED` row whose
/// `proj_id` (the successor's project, which owns the row) differs from its
/// `pred_proj_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalLink {
    /// `task_pred_id`.
    pub row_id: i64,
    pub pred_project: i64,
    pub pred_task: i64,
    pub succ_project: i64,
    pub succ_task: i64,
    pub link_type: LinkType,
    /// Lag in minutes.
    pub lag: i64,
}
#[derive(Clone, Debug)]
pub struct ProjectData {
    pub model: Model,
    pub details: BTreeMap<i64, ActivityDetails>,
    pub wbs: Vec<Wbs>,
    pub resource_names: BTreeMap<i64, String>,
    pub calendar_names: Vec<String>,
    pub document: Document,
    pub source_name: String,
    pub warnings: Vec<String>,
    pub resources: LevelingInputs,
}
fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
}
fn ascii_digits(b: &[u8]) -> Option<u32> {
    let mut v = 0_u32;
    for &c in b {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + u32::from(c - b'0');
    }
    Some(v)
}
pub fn parse_absolute(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let b = s.as_bytes();
    let fast = match b.len() {
        16 if b[4] == b'-' && b[7] == b'-' && b[10] == b' ' && b[13] == b':' => {
            if let (Some(y), Some(m), Some(d), Some(hh), Some(mm)) = (
                ascii_digits(&b[0..4]),
                ascii_digits(&b[5..7]),
                ascii_digits(&b[8..10]),
                ascii_digits(&b[11..13]),
                ascii_digits(&b[14..16]),
            ) {
                NaiveDate::from_ymd_opt(y as i32, m, d).and_then(|dt| dt.and_hms_opt(hh, mm, 0))
            } else {
                None
            }
        }
        19 if b[4] == b'-' && b[7] == b'-' && b[10] == b' ' && b[13] == b':' && b[16] == b':' => {
            if let (Some(y), Some(m), Some(d), Some(hh), Some(mm), Some(ss)) = (
                ascii_digits(&b[0..4]),
                ascii_digits(&b[5..7]),
                ascii_digits(&b[8..10]),
                ascii_digits(&b[11..13]),
                ascii_digits(&b[14..16]),
                ascii_digits(&b[17..19]),
            ) {
                NaiveDate::from_ymd_opt(y as i32, m, d).and_then(|dt| dt.and_hms_opt(hh, mm, ss))
            } else {
                None
            }
        }
        10 if b[4] == b'-' && b[7] == b'-' => {
            if let (Some(y), Some(m), Some(d)) = (
                ascii_digits(&b[0..4]),
                ascii_digits(&b[5..7]),
                ascii_digits(&b[8..10]),
            ) {
                NaiveDate::from_ymd_opt(y as i32, m, d).and_then(|dt| dt.and_hms_opt(0, 0, 0))
            } else {
                None
            }
        }
        _ => None,
    };
    let d = match fast {
        Some(d) => d,
        None => NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M"))
            .or_else(|_| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            })
            .ok()?,
    };
    Some((d - epoch()).num_minutes())
}
pub fn date_string(origin: i64, value: Option<i64>) -> String {
    value
        .and_then(|v| epoch().checked_add_signed(Duration::minutes(origin * DAY + v)))
        .map_or(String::new(), |d| {
            let y = d.year();
            if (0..=9999).contains(&y) {
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    y,
                    d.month(),
                    d.day(),
                    d.hour(),
                    d.minute()
                )
            } else {
                d.format("%Y-%m-%d %H:%M").to_string()
            }
        })
}
pub fn short_date(origin: i64, value: Option<i64>) -> String {
    value
        .and_then(|v| epoch().checked_add_signed(Duration::minutes(origin * DAY + v)))
        .map_or(String::new(), |d| d.format("%d-%b-%y").to_string())
}
pub fn day_label(origin: i64, value: i64, fmt: &str) -> String {
    epoch()
        .checked_add_signed(Duration::minutes(origin * DAY + value))
        .map_or(String::new(), |d| d.format(fmt).to_string())
}
pub fn parse_relative(origin: i64, s: &str) -> Result<Option<i64>, String> {
    if s.trim().is_empty() {
        Ok(None)
    } else {
        parse_absolute(s)
            .map(|v| Some(v - origin * DAY))
            .ok_or_else(|| "Use YYYY-MM-DD or YYYY-MM-DD HH:MM".into())
    }
}
fn num(s: &str) -> i64 {
    s.parse::<i64>().unwrap_or(0)
}
fn hours(s: &str) -> i64 {
    (s.parse::<f64>().unwrap_or(0.0) * 60.0).round_ties_even() as i64
}
/// Hours as the reference writes them: up to eight decimals without trailing zeros.
fn out_hours(v: Option<i64>) -> String {
    v.map_or(String::new(), |n| {
        let text = format!("{:.8}", n as f64 / 60.0);
        let text = text.trim_end_matches('0').trim_end_matches('.');
        if text == "-0" {
            "0".into()
        } else {
            text.into()
        }
    })
}
fn constraint(s: &str) -> CstrType {
    match s {
        "CS_MSO" => CstrType::StartOn,
        "CS_MSOB" => CstrType::StartOnBefore,
        "CS_MSOA" => CstrType::StartOnAfter,
        "CS_ALAP" => CstrType::Alap,
        "CS_MEO" => CstrType::FinishOn,
        "CS_MEOB" => CstrType::FinishOnBefore,
        "CS_MEOA" => CstrType::FinishOnAfter,
        "CS_MANDSTART" => CstrType::MandatoryStart,
        "CS_MANDFIN" => CstrType::MandatoryFinish,
        _ => CstrType::None,
    }
}
pub fn constraint_code(c: CstrType) -> &'static str {
    match c {
        CstrType::None => "",
        CstrType::StartOn => "CS_MSO",
        CstrType::StartOnBefore => "CS_MSOB",
        CstrType::StartOnAfter => "CS_MSOA",
        CstrType::Alap => "CS_ALAP",
        CstrType::FinishOn => "CS_MEO",
        CstrType::FinishOnBefore => "CS_MEOB",
        CstrType::FinishOnAfter => "CS_MEOA",
        CstrType::MandatoryStart => "CS_MANDSTART",
        CstrType::MandatoryFinish => "CS_MANDFIN",
    }
}
pub fn constraint_label(c: CstrType) -> &'static str {
    match c {
        CstrType::None => "None",
        CstrType::StartOn => "Start On",
        CstrType::StartOnBefore => "Start On or Before",
        CstrType::StartOnAfter => "Start On or After",
        CstrType::Alap => "As Late As Possible",
        CstrType::FinishOn => "Finish On",
        CstrType::FinishOnBefore => "Finish On or Before",
        CstrType::FinishOnAfter => "Finish On or After",
        CstrType::MandatoryStart => "Mandatory Start",
        CstrType::MandatoryFinish => "Mandatory Finish",
    }
}
pub fn type_label(t: TaskType) -> &'static str {
    match t {
        TaskType::Task => "Task Dependent",
        TaskType::Mile => "Start Milestone",
        TaskType::FinMile => "Finish Milestone",
        TaskType::Loe => "Level of Effort",
        TaskType::Wbs => "WBS Summary",
        TaskType::Hamwk => "Hammock",
        TaskType::Resource => "Resource Dependent",
    }
}
pub fn status_label(t: StatusType) -> &'static str {
    match t {
        StatusType::NotStarted => "Not Started",
        StatusType::InProgress => "In Progress",
        StatusType::Completed => "Completed",
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Node {
    name: String,
    attrs: HashMap<String, String>,
    /// Attribute keys in their written order.
    order: Vec<String>,
    children: Vec<Node>,
}
/// Calendar trees are a few levels deep; deeper nesting is malformed.
const MAX_CALENDAR_DEPTH: usize = 32;
/// Reads one calendar node with the shared tree parser (`tree::Tree`).
fn node(data: &str, pos: &mut usize) -> Result<Node, String> {
    let tree = tree::Tree::parse_at(data, pos, MAX_CALENDAR_DEPTH + 1)
        .map_err(|e| format!("Invalid calendar tree: {e}"))?;
    Ok(Node::from_tree(tree))
}
impl Node {
    fn from_tree(t: tree::Tree) -> Node {
        let order = t.pairs().map(|(k, _)| k.to_string()).collect();
        let attrs = t.pairs().map(|(k, v)| (k.into(), v.into())).collect();
        Node {
            name: t.name.trim().to_string(),
            attrs,
            order,
            children: t.children.into_iter().map(Node::from_tree).collect(),
        }
    }
}
/// A shift time `HH:MM`, checked to lie within the day as the calendar
/// editor checks it (00:00 to 24:59), so no value can overflow or put a
/// shift outside its day.
fn hm(s: &str) -> Result<i64, String> {
    calendar_data::shift_minutes(s)
}
fn shifts(n: &Node) -> Result<Shifts, String> {
    n.children
        .iter()
        .map(|c| {
            Ok((
                hm(c.attrs.get("s").ok_or("Shift start missing")?)?,
                hm(c.attrs.get("f").ok_or("Shift finish missing")?)?,
            ))
        })
        .collect()
}
fn calendar(id: &str, blob: &str, origin: i64) -> Result<CalendarAdapter, String> {
    let normalized = blob.replace("\u{7f}\u{7f}", "\r\n").replace('\u{7f}', "\n");
    let root = node(normalized.trim(), &mut 0)?;
    let mut week: [Shifts; 7] = std::array::from_fn(|_| vec![]);
    let mut exc = BTreeMap::new();
    for n in root.children {
        if n.name == "DaysOfWeek" {
            for day in n.children {
                let index = day.name.parse::<usize>().map_err(|_| "Invalid weekday")?;
                if !(1..=7).contains(&index) {
                    return Err("Weekday outside 1..7".into());
                }
                week[index - 1] = shifts(&day)?;
            }
        } else if n.name == "Exceptions" {
            for day in n.children {
                exc.insert(
                    day.attrs
                        .get("d")
                        .ok_or("Exception date missing")?
                        .parse::<i64>()
                        .map_err(|_| "Invalid exception date")?,
                    shifts(&day)?,
                );
            }
        }
    }
    let date = epoch()
        .checked_add_signed(Duration::try_days(origin).ok_or("Calendar origin is out of range")?)
        .ok_or("Calendar origin is out of range")?
        .date();
    let first = date.weekday().num_days_from_sunday() as usize;
    let template = std::array::from_fn(|i| week[(first + i) % 7].clone());
    let continuous = week
        .iter()
        .all(|s| s.len() == 1 && s[0].0 == 0 && (s[0].1 == 0 || s[0].1 == 1440))
        && exc.is_empty();
    Ok(CalendarAdapter::new(WorkCalendar::new(
        id.into(),
        continuous,
        origin,
        template,
        exc,
    )))
}
/// Durations, lags and floats beyond this many hours (about 1,100 years) are
/// rejected on import instead of overflowing the scheduler's minute arithmetic.
const MAX_HOURS: f64 = 1.0e7;
fn parse_hours(text: &str) -> Result<Option<i64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let value: f64 = text
        .parse()
        .map_err(|_| format!("\"{text}\" is not a number"))?;
    if !value.is_finite() || value.abs() > MAX_HOURS {
        return Err(format!("{text} hours is out of range"));
    }
    Ok(Some((value * 60.0).round_ties_even() as i64))
}
/// Reject assignments whose lags, or durations taken from units / units per
/// hour, are beyond [`MAX_HOURS`], as activity durations are: the scheduler
/// would otherwise walk the calendar for centuries. Text that is not a number
/// keeps its old reading (zero).
fn check_assignment_hours(doc: &Document, pid: i64, activities: &[Activity]) -> Result<(), String> {
    let Some(t) = doc.table("TASKRSRC") else {
        return Ok(());
    };
    let cols = t.field_index();
    let proj_c = cols.col("proj_id");
    let task_c = cols.col("task_id");
    let id_c = cols.col("taskrsrc_id");
    let rq_c = cols.col("remain_qty");
    let rr_c = cols.col("remain_qty_per_hr");
    let tq_c = cols.col("target_qty");
    let tr_c = cols.col("target_qty_per_hr");
    let relag_c = cols.col("relag_drtn_hr_cnt");
    let tlag_c = cols.col("target_lag_drtn_hr_cnt");
    let number = |text: &str| text.trim().parse::<f64>().ok();
    for r in &t.rows {
        if num(FieldIndex::get(r, proj_c)) != pid {
            continue;
        }
        let what = || {
            let task = num(FieldIndex::get(r, task_c));
            let code = activities
                .iter()
                .find(|a| a.task_id == task)
                .map_or_else(|| task.to_string(), |a| a.task_code.clone());
            format!(
                "Assignment {} of activity {code}",
                FieldIndex::get(r, id_c).trim()
            )
        };
        for (qty, rate, label) in [(rq_c, rr_c, "remaining"), (tq_c, tr_c, "budgeted")] {
            let (Some(q), Some(h)) = (
                number(FieldIndex::get(r, qty)),
                number(FieldIndex::get(r, rate)),
            ) else {
                continue;
            };
            if h != 0. && (!(q / h).is_finite() || (q / h).abs() > MAX_HOURS) {
                return Err(format!(
                    "{} {label} units / units per hour ({q} / {h}) is out of range",
                    what()
                ));
            }
        }
        for (column, label) in [(relag_c, "lag"), (tlag_c, "planned lag")] {
            if let Some(v) = number(FieldIndex::get(r, column)) {
                if !v.is_finite() || v.abs() > MAX_HOURS {
                    return Err(format!("{} {label}: {v} hours is out of range", what()));
                }
            }
        }
    }
    Ok(())
}
fn task_type(s: &str) -> Option<TaskType> {
    Some(match s {
        "TT_Task" => TaskType::Task,
        "TT_Mile" => TaskType::Mile,
        "TT_FinMile" => TaskType::FinMile,
        "TT_LOE" => TaskType::Loe,
        "TT_WBS" => TaskType::Wbs,
        "TT_Hamwk" => TaskType::Hamwk,
        "TT_Rsrc" => TaskType::Resource,
        _ => return None,
    })
}
fn status(s: &str) -> StatusType {
    match s {
        "TK_Active" => StatusType::InProgress,
        "TK_Complete" => StatusType::Completed,
        _ => StatusType::NotStarted,
    }
}
fn link_type(s: &str) -> LinkType {
    match s {
        "PR_SS" => LinkType::SS,
        "PR_FF" => LinkType::FF,
        "PR_SF" => LinkType::SF,
        _ => LinkType::FS,
    }
}
fn optional_hours(s: &str) -> Option<i64> {
    if s.is_empty() {
        None
    } else {
        Some(hours(s))
    }
}
fn flag_or(s: &str, default: bool) -> bool {
    if s.is_empty() {
        default
    } else {
        s == "Y"
    }
}
fn yes_no(v: bool) -> String {
    if v { "Y" } else { "N" }.into()
}
fn float_type(s: &str) -> FloatType {
    match s {
        "FT_SS" => FloatType::Start,
        "FT_Min" => FloatType::Min,
        _ => FloatType::Finish,
    }
}
fn lag_calendar(s: &str) -> LagCalendar {
    match s {
        "rcal_Successor" => LagCalendar::SuccCal,
        "rcal_24Hour" => LagCalendar::Hour24,
        "rcal_ProjDefault" => LagCalendar::ProjCal,
        _ => LagCalendar::PredCal,
    }
}
fn out_of_sequence(s: &str) -> OutOfSequence {
    if s == "SD_None" {
        OutOfSequence::None
    } else {
        OutOfSequence::Both
    }
}
/// The number of float paths to calculate; the engine needs at least one,
/// so zero or text that is not a count reads as the default.
fn max_paths(s: &str) -> usize {
    read_max_paths(s).unwrap_or(10)
}
fn read_max_paths(s: &str) -> Option<usize> {
    s.trim().parse().ok().filter(|&n| n > 0)
}
fn key_activity(s: &str) -> Option<i64> {
    s.parse::<i64>().ok().filter(|&n| n > 0)
}
/// Options used when a project has no SCHEDOPTIONS row.
fn default_options() -> ScheduleOptions {
    ScheduleOptions {
        retained_logic: true,
        lag_from_early_start: true,
        use_expected_finish: true,
        float_type: FloatType::Finish,
        out_of_sequence: OutOfSequence::RetainedLogic,
        open_end_uses_early: false,
        ..ScheduleOptions::default()
    }
}
fn schedule_options(t: &Table, r: &[String]) -> ScheduleOptions {
    let v = |k| t.value(r, k);
    ScheduleOptions {
        retained_logic: flag_or(v("sched_retained_logic"), true),
        progress_override: flag_or(v("sched_progress_override"), false),
        lag_from_early_start: flag_or(v("sched_lag_early_start_flag"), true),
        use_expected_finish: flag_or(v("sched_use_expect_end_flag"), true),
        use_must_finish: flag_or(v("sched_use_project_end_date_for_float"), true),
        open_end_uses_early: flag_or(v("sched_open_critical_flag"), false),
        float_type: float_type(v("sched_float_type")),
        lag_calendar: lag_calendar(v("sched_calendar_on_relationship_lag")),
        out_of_sequence: out_of_sequence(v("sched_outer_depend_type")),
        multiple_float_paths: v("enable_multiple_longest_path_calc") == "Y",
        float_paths_use_total: v("use_total_float_multiple_longest_paths") != "N",
        max_float_paths: max_paths(v("max_multiple_longest_path")),
        float_paths_end_activity: key_activity(v("key_activity_for_multiple_longest_paths")),
        ..default_options()
    }
}
/// A short list of example activity IDs for a warning.
fn examples(codes: &[String]) -> String {
    let mut out = codes.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    if codes.len() > 5 {
        out.push_str(", ...");
    }
    out
}
/// Writes the owned columns of one row. A cell keeps its original text when that
/// text already imports as the value being written, so an unchanged value is
/// never reformatted. Fresh rows are written in full.
pub(crate) struct RowWriter<'a> {
    pub(crate) table: &'a mut Table,
    pub(crate) cols: &'a mut FieldIndex,
    pub(crate) row: usize,
    pub(crate) fresh: bool,
}
impl RowWriter<'_> {
    pub(crate) fn old(&self, field: &str) -> &str {
        self.cols.value(&self.table.rows[self.row], field)
    }
    pub(crate) fn put(
        &mut self,
        field: &str,
        keeps: impl FnOnce(&str) -> bool,
        value: impl FnOnce() -> String,
    ) -> bool {
        let old = self.old(field);
        if !self.fresh && keeps(old) {
            return false;
        }
        let value = value();
        if old == value || (value.is_empty() && self.cols.col(field).is_none()) {
            return false;
        }
        self.cols.set(self.table, self.row, field, value);
        true
    }
    /// Fill a blank cell of a fresh row with the value the reference writes for new rows.
    pub(crate) fn default(&mut self, field: &str, value: &str) {
        if self.cols.col(field).is_some() && self.old(field).is_empty() {
            self.cols.set(self.table, self.row, field, value.into());
        }
    }
}
/// A new 22-character base64 GUID in the form the reference writes.
pub fn new_guid(seed: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    let state = std::collections::hash_map::RandomState::new();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut bytes = [0u8; 16];
    for (half, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut h = state.build_hasher();
        h.write(seed.as_bytes());
        h.write_u128(nanos);
        h.write_usize(half);
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = String::with_capacity(22);
    for b in bytes {
        bits = (bits << 8) | u32::from(b);
        count += 8;
        while count >= 6 {
            count -= 6;
            out.push(ALPHABET[((bits >> count) & 63) as usize] as char);
        }
    }
    if count > 0 {
        out.push(ALPHABET[((bits << (6 - count)) & 63) as usize] as char);
    }
    out
}
/// Values the reference writes for a newly added activity in columns OpenXER does not
/// calculate.
const NEW_TASK_DEFAULTS: &[(&str, &str)] = &[
    ("phys_complete_pct", "0"),
    ("rev_fdbk_flag", "N"),
    ("est_wt", "1"),
    ("lock_plan_flag", "N"),
    ("auto_compute_act_flag", "N"),
    ("complete_pct_type", "CP_Drtn"),
    ("duration_type", "DT_FixedDUR2"),
    ("priority_type", "PT_Normal"),
    ("act_work_qty", "0"),
    ("remain_work_qty", "0"),
    ("target_work_qty", "0"),
    ("target_equip_qty", "0"),
    ("act_equip_qty", "0"),
    ("remain_equip_qty", "0"),
    ("act_this_per_work_qty", "0"),
    ("act_this_per_equip_qty", "0"),
    ("control_updates_flag", "N"),
    ("review_type", "RV_OK"),
];
fn now_stamp() -> String {
    chrono::Local::now()
        .naive_local()
        .format("%Y-%m-%d %H:%M")
        .to_string()
}
impl ProjectData {
    pub fn from_document(
        document: Document,
        project_id: i64,
        source_name: String,
    ) -> Result<Self, String> {
        let mut warnings = document.warnings.clone();
        let p = document
            .table("PROJECT")
            .ok_or("The file has no PROJECT table, so it contains no projects")?;
        let mut project_rows = p
            .rows
            .iter()
            .filter(|r| num(p.value(r, "proj_id")) == project_id);
        let row = project_rows
            .next()
            .ok_or_else(|| format!("Project {project_id} is not in this file"))?;
        if project_rows.next().is_some() {
            warnings.push(format!(
                "Project {project_id} appears on more than one PROJECT row; the first is used."
            ));
        }
        let pv = |k| p.value(row, k);
        let dd = parse_absolute(pv("last_recalc_date"))
            .or_else(|| parse_absolute(pv("plan_start_date")))
            .ok_or_else(|| {
                format!(
                    "Project {} has no data date or planned start",
                    pv("proj_short_name")
                )
            })?;
        let empty = Table {
            name: "TASK".into(),
            fields: vec![],
            rows: vec![],
        };
        let tasks = document.table("TASK").unwrap_or(&empty);
        let tcols = tasks.field_index();
        let t_proj = tcols.col("proj_id");
        let t_act_start = tcols.col("act_start_date");
        let t_act_end = tcols.col("act_end_date");
        let t_clndr = tcols.col("clndr_id");
        let rows: Vec<_> = tasks
            .rows
            .iter()
            .filter(|r| num(FieldIndex::get(r, t_proj)) == project_id)
            .collect();
        let earliest = rows
            .iter()
            .flat_map(|r| {
                [
                    parse_absolute(FieldIndex::get(r, t_act_start)),
                    parse_absolute(FieldIndex::get(r, t_act_end)),
                ]
            })
            .flatten()
            .min()
            .unwrap_or(dd);
        let origin_serial = (dd - 40 * DAY).min(earliest - DAY).div_euclid(DAY);
        let relative = |s: &str| parse_absolute(s).map(|v| v - origin_serial * DAY);
        let used_calendars: std::collections::HashSet<&str> = rows
            .iter()
            .map(|r| FieldIndex::get(r, t_clndr))
            .chain([pv("clndr_id")])
            .collect();
        let mut calendars = vec![];
        let mut calendar_names = vec![];
        let mut cal_index = HashMap::new();
        let mut default_calendar = None;
        if let Some(cals) = document.table("CALENDAR") {
            for r in &cals.rows {
                let id = cals.value(r, "clndr_id");
                let name = cals.value(r, "clndr_name");
                if cal_index.contains_key(id) {
                    warnings.push(format!(
                        "Calendar ID {id} appears more than once; the first ({}) is used.",
                        calendar_names[cal_index[id]]
                    ));
                    continue;
                }
                let c = match calendar(id, cals.value(r, "clndr_data"), origin_serial) {
                    Ok(c) => c,
                    Err(e) if used_calendars.contains(id) => {
                        return Err(format!("Calendar {id} ({name}): {e}"))
                    }
                    Err(e) => {
                        warnings.push(format!(
                            "Calendar {id} ({name}) could not be read and is not used by this project: {e}"
                        ));
                        continue;
                    }
                };
                if default_calendar.is_none() && cals.value(r, "default_flag") == "Y" {
                    default_calendar = Some(calendars.len());
                }
                cal_index.insert(id.to_string(), calendars.len());
                calendars.push(c);
                calendar_names.push(name.to_string());
            }
        }
        let project_calendar = match cal_index.get(pv("clndr_id")) {
            Some(&i) => i,
            None if calendars.is_empty() => {
                if !rows.is_empty() {
                    return Err(
                        "The file has no calendars, so its activities cannot be scheduled".into(),
                    );
                }
                0
            }
            None => {
                let i = default_calendar.unwrap_or(0);
                warnings.push(format!(
                    "Project calendar {} is not in the file; calendar {} ({}) is used instead.",
                    pv("clndr_id"),
                    calendars[i].id,
                    calendar_names[i]
                ));
                i
            }
        };
        let wbs_ids: std::collections::HashSet<i64> = document
            .table("PROJWBS")
            .map(|t| {
                let cols = t.field_index();
                let proj_c = cols.col("proj_id");
                let wbs_c = cols.col("wbs_id");
                t.rows
                    .iter()
                    .filter(|r| num(FieldIndex::get(r, proj_c)) == project_id)
                    .map(|r| num(FieldIndex::get(r, wbs_c)))
                    .collect()
            })
            .unwrap_or_default();
        let mut activities = Vec::with_capacity(rows.len());
        let mut details = BTreeMap::new();
        let mut indices = HashMap::with_capacity(rows.len());
        let mut codes: HashMap<String, usize> = HashMap::with_capacity(rows.len());
        let mut missing_calendar = vec![];
        let mut missing_wbs = vec![];
        let mut duplicate_codes = vec![];
        let c_task_id = tcols.col("task_id");
        let c_task_code = tcols.col("task_code");
        let c_task_type = tcols.col("task_type");
        let c_clndr_id = tcols.col("clndr_id");
        let c_wbs_id = tcols.col("wbs_id");
        let c_cstr_type = tcols.col("cstr_type");
        let c_cstr_date = tcols.col("cstr_date");
        let c_cstr_type2 = tcols.col("cstr_type2");
        let c_cstr_date2 = tcols.col("cstr_date2");
        let c_status_code = tcols.col("status_code");
        let c_act_start = tcols.col("act_start_date");
        let c_act_end = tcols.col("act_end_date");
        let c_resume = tcols.col("resume_date");
        let c_expect_end = tcols.col("expect_end_date");
        let c_rem = tcols.col("remain_drtn_hr_cnt");
        let c_ext_es = tcols.col("external_early_start_date");
        let c_ext_le = tcols.col("external_late_end_date");
        let c_restart = tcols.col("restart_date");
        let c_reend = tcols.col("reend_date");
        let c_es = tcols.col("early_start_date");
        let c_ef = tcols.col("early_end_date");
        let c_ls = tcols.col("late_start_date");
        let c_lf = tcols.col("late_end_date");
        let c_rls = tcols.col("rem_late_start_date");
        let c_rlf = tcols.col("rem_late_end_date");
        let c_tf = tcols.col("total_float_hr_cnt");
        let c_ff = tcols.col("free_float_hr_cnt");
        let c_drv = tcols.col("driving_path_flag");
        let c_ts = tcols.col("target_start_date");
        let c_te = tcols.col("target_end_date");
        let c_fp = tcols.col("float_path");
        let c_fpo = tcols.col("float_path_order");
        let c_name = tcols.col("task_name");
        let c_orig_dur = tcols.col("target_drtn_hr_cnt");
        let c_pct = tcols.col("phys_complete_pct");
        for r in rows {
            let v = |c: Option<usize>| FieldIndex::get(r, c);
            let id = num(v(c_task_id));
            let code = v(c_task_code);
            if let Some(&other) = indices.get(&id) {
                let other: &Activity = &activities[other];
                return Err(format!(
                    "Activity ID {id} is used by more than one TASK row ({} and {code})",
                    other.task_code
                ));
            }
            let task_type = task_type(v(c_task_type)).ok_or_else(|| {
                format!("Activity {code} uses unsupported type {}", v(c_task_type))
            })?;
            let duration = |c: Option<usize>, what: &str| {
                parse_hours(v(c))
                    .map(Option::unwrap_or_default)
                    .map_err(|e| format!("Activity {code} {what}: {e}"))
            };
            let float = |c: Option<usize>| parse_hours(v(c)).ok().flatten();
            let cal = match cal_index.get(v(c_clndr_id)) {
                Some(&i) => i,
                None => {
                    missing_calendar.push(code.to_string());
                    project_calendar
                }
            };
            if !wbs_ids.contains(&num(v(c_wbs_id))) {
                missing_wbs.push(code.to_string());
            }
            if codes.insert(code.to_string(), activities.len()).is_some() {
                duplicate_codes.push(code.to_string());
            }
            let has_primary = !v(c_cstr_type).is_empty();
            let restart_val = relative(v(c_restart));
            let reend_val = relative(v(c_reend));
            let a = Activity {
                task_id: id,
                task_code: code.into(),
                task_type,
                status: status(v(c_status_code)),
                act_start: relative(v(c_act_start)),
                act_finish: relative(v(c_act_end)),
                resume: relative(v(c_resume)),
                expect_end: relative(v(c_expect_end)),
                cstr: constraint(v(c_cstr_type)),
                cstr_date: relative(v(c_cstr_date)),
                cstr2: if has_primary {
                    constraint(v(c_cstr_type2))
                } else {
                    CstrType::None
                },
                cstr_date2: if has_primary {
                    relative(v(c_cstr_date2))
                } else {
                    None
                },
                rem: duration(c_rem, "remaining duration")?,
                cal,
                ext_early_start: relative(v(c_ext_es)),
                ext_late_end: relative(v(c_ext_le)),
                rem_restart: restart_val,
                rem_reend: reend_val,
                restart: restart_val,
                reend: reend_val,
                early_start: relative(v(c_es)),
                early_finish: relative(v(c_ef)),
                late_start: relative(v(c_ls)),
                late_finish: relative(v(c_lf)),
                rem_late_start: relative(v(c_rls)),
                rem_late_finish: relative(v(c_rlf)),
                total_float: float(c_tf),
                free_float: float(c_ff),
                driving_path: v(c_drv) == "Y",
                target_start: relative(v(c_ts)),
                target_finish: relative(v(c_te)),
                float_path: num(v(c_fp)),
                float_path_order: num(v(c_fpo)),
                ..Activity::default()
            };
            details.insert(
                id,
                ActivityDetails {
                    name: v(c_name).into(),
                    wbs: num(v(c_wbs_id)),
                    original_duration: duration(c_orig_dur, "original duration")?,
                    // Set from the assignments below.
                    resource_driven_duration: false,
                    percent: v(c_pct).parse().unwrap_or(0.0),
                    minted: None,
                },
            );
            indices.insert(id, activities.len());
            activities.push(a);
        }
        if !missing_calendar.is_empty() {
            warnings.push(format!(
                "{} activities use a calendar that is not in the file ({}); they use the project calendar.",
                missing_calendar.len(),
                examples(&missing_calendar)
            ));
        }
        if !missing_wbs.is_empty() {
            warnings.push(format!(
                "{} activities belong to a WBS element that is not in the file ({}); they are shown as Unassigned.",
                missing_wbs.len(),
                examples(&missing_wbs)
            ));
        }
        if !duplicate_codes.is_empty() {
            warnings.push(format!(
                "{} activity IDs are used more than once ({}).",
                duplicate_codes.len(),
                examples(&duplicate_codes)
            ));
        }
        let mut links = vec![];
        let mut external = 0;
        let mut dangling = 0;
        if let Some(table) = document.table("TASKPRED") {
            let pcols = table.field_index();
            let p_proj = pcols.col("proj_id");
            let p_pred_proj = pcols.col("pred_proj_id");
            let p_pred_task = pcols.col("pred_task_id");
            let p_task = pcols.col("task_id");
            let p_lag = pcols.col("lag_hr_cnt");
            let p_id = pcols.col("task_pred_id");
            let p_type = pcols.col("pred_type");
            for r in &table.rows {
                let succ_project = num(FieldIndex::get(r, p_proj));
                let pred_project = num(FieldIndex::get(r, p_pred_proj));
                if succ_project != project_id && pred_project != project_id {
                    continue;
                }
                // A row naming another project at either end is a relationship
                // with that project even when the activity ids also occur in
                // this one (two projects made apart can both hold id 1010).
                // A blank project id reads as this project, as before.
                let ours = |col: Option<usize>| {
                    let v = FieldIndex::get(r, col).trim();
                    v.is_empty() || num(v) == project_id
                };
                let inside = ours(p_proj) && ours(p_pred_proj);
                let pred = indices.get(&num(FieldIndex::get(r, p_pred_task)));
                let succ = indices.get(&num(FieldIndex::get(r, p_task)));
                if let (Some(&pred), Some(&succ), true) = (pred, succ, inside) {
                    let lag = parse_hours(FieldIndex::get(r, p_lag))
                        .map_err(|e| format!("Relationship {} lag: {e}", FieldIndex::get(r, p_id)))?
                        .unwrap_or(0);
                    links.push(Link {
                        pred,
                        succ,
                        link_type: link_type(FieldIndex::get(r, p_type)),
                        lag,
                        driving_path: false,
                    });
                } else if succ_project == project_id && pred_project == project_id {
                    dangling += 1;
                } else {
                    external += 1;
                }
            }
        }
        check_assignment_hours(&document, project_id, &activities)?;
        assignments::apply_rows(
            &document,
            project_id,
            &mut activities,
            &calendars,
            &mut details,
        );
        let options_row = document.table("SCHEDOPTIONS").and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == project_id)
                .map(|r| (t, r))
        });
        let options = options_row
            .map(|(t, r)| schedule_options(t, r))
            .unwrap_or_else(default_options);
        // The engine refuses a path count below one; such a count reads as
        // the default. `write_options` leaves the stored cell as it was.
        if let Some((t, r)) = options_row {
            let count = t.value(r, "max_multiple_longest_path");
            if !count.trim().is_empty() && read_max_paths(count).is_none() {
                warnings.push(format!(
                    "The schedule options give {:?} as the number of float paths to calculate; it is read as {}.",
                    count.trim(),
                    max_paths(count)
                ));
            }
        }
        // The engine refuses a float-path ending activity the project does
        // not hold; read it as blank so the file still opens and schedules.
        // `write_options` leaves the stored cell as it was.
        let mut options = options;
        if options
            .float_paths_end_activity
            .is_some_and(|id| !indices.contains_key(&id))
        {
            options.float_paths_end_activity = None;
            warnings.push(
                "The schedule options end multiple float paths with an activity that is not in the file; the ending activity is read as blank."
                    .into(),
            );
        }
        let project = ProjectSettings {
            proj_id: project_id,
            name: pv("proj_short_name").into(),
            data_date: Some(dd - origin_serial * DAY),
            scd_end: relative(pv("scd_end_date")),
            plan_start: relative(pv("plan_start_date")),
            plan_end: relative(pv("plan_end_date")),
            last_schedule: relative(pv("last_schedule_date")),
        };
        let mut model = Model {
            origin_serial,
            project_calendar,
            calendars,
            activities,
            links,
            project,
            options,
        };
        model.rebuild_links()?;
        preprocess_units(&document, &mut model, &mut details, true);
        // The engine refuses a negative remaining duration; read it as zero so
        // the file still opens and schedules.
        let negative: Vec<String> = model
            .activities
            .iter_mut()
            .filter(|a| a.rem < 0)
            .map(|a| {
                a.rem = 0;
                a.task_code.clone()
            })
            .collect();
        if !negative.is_empty() {
            warnings.push(format!(
                "{} activities have a negative remaining duration ({}); zero is used.",
                negative.len(),
                examples(&negative)
            ));
        }
        let mut wbs = vec![];
        if let Some(t) = document.table("PROJWBS") {
            let cols = t.field_index();
            let c_proj = cols.col("proj_id");
            let c_id = cols.col("wbs_id");
            let c_parent = cols.col("parent_wbs_id");
            let c_short = cols.col("wbs_short_name");
            let c_name = cols.col("wbs_name");
            let c_seq = cols.col("seq_num");
            for r in &t.rows {
                if num(FieldIndex::get(r, c_proj)) == project_id {
                    wbs.push(Wbs {
                        id: num(FieldIndex::get(r, c_id)),
                        parent: FieldIndex::get(r, c_parent).parse().ok(),
                        code: FieldIndex::get(r, c_short).into(),
                        name: FieldIndex::get(r, c_name).into(),
                        sequence: num(FieldIndex::get(r, c_seq)),
                    });
                }
            }
        }
        wbs.sort_by_key(|w| (w.sequence, w.id));
        break_wbs_cycles(&mut wbs, &mut warnings);
        let resource_names = document.table("RSRC").map_or(BTreeMap::new(), |t| {
            t.rows
                .iter()
                .map(|r| (num(t.value(r, "rsrc_id")), t.value(r, "rsrc_name").into()))
                .collect()
        });
        if external > 0 {
            warnings.push(format!(
                "{external} cross-project relationships use stored external date limits."
            ));
        }
        if dangling > 0 {
            warnings.push(format!(
                "{dangling} relationships refer to activities that are not in the file; they were ignored and are kept unchanged on export."
            ));
        }
        let resources = leveling_inputs(&document, &model, &wbs);
        Ok(Self {
            model,
            details,
            wbs,
            resource_names,
            calendar_names,
            document,
            source_name,
            warnings,
            resources,
        })
    }
    /// The comment of each relationship in `links` (model link indices): the
    /// `TASKPRED` row of the same pair and type, or the pair's only row. A
    /// relationship made in this session has no row yet and reads blank.
    pub fn relationship_comments(&self, links: &[usize]) -> Vec<String> {
        let Some(t) = self.document.table("TASKPRED") else {
            return vec![String::new(); links.len()];
        };
        let Some(comments) = t.field("comments") else {
            return vec![String::new(); links.len()];
        };
        let (task, pred, kind, proj) = (
            t.field("task_id"),
            t.field("pred_task_id"),
            t.field("pred_type"),
            t.field("proj_id"),
        );
        let cell = |r: &Vec<String>, c: Option<usize>| -> String {
            c.and_then(|c| r.get(c)).cloned().unwrap_or_default()
        };
        let m = &self.model;
        let wanted: std::collections::HashMap<(i64, i64), Vec<usize>> = {
            let mut map: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
            for (k, &l) in links.iter().enumerate() {
                if let Some(l) = m.links.get(l) {
                    map.entry((m.activities[l.pred].task_id, m.activities[l.succ].task_id))
                        .or_default()
                        .push(k);
                }
            }
            map
        };
        // Rows per wanted pair: (type, comment), in file order.
        let mut found: std::collections::HashMap<(i64, i64), Vec<(String, String)>> =
            Default::default();
        for r in &t.rows {
            if num(&cell(r, proj)) != m.project.proj_id {
                continue;
            }
            let key = (num(&cell(r, pred)), num(&cell(r, task)));
            if wanted.contains_key(&key) {
                found
                    .entry(key)
                    .or_default()
                    .push((cell(r, kind), cell(r, Some(comments))));
            }
        }
        let mut out = vec![String::new(); links.len()];
        for (key, ks) in &wanted {
            let Some(rows) = found.get(key) else {
                continue;
            };
            for &k in ks {
                let l = &m.links[links[k]];
                out[k] = rows
                    .iter()
                    .find(|(t, _)| t == l.link_type.as_str())
                    .or_else(|| (rows.len() == 1).then_some(&rows[0]))
                    .map(|(_, c)| c.clone())
                    .unwrap_or_default();
            }
        }
        out
    }
    /// The comment of the `TASKPRED` row `row_id` (a relationship with another project).
    pub fn relationship_comment_of_row(&self, row_id: i64) -> String {
        let Some(t) = self.document.table("TASKPRED") else {
            return String::new();
        };
        t.rows
            .iter()
            .find(|r| num(t.value(r, "task_pred_id")) == row_id)
            .map(|r| t.value(r, "comments").to_string())
            .unwrap_or_default()
    }
    /// Relationships this project owns whose predecessor is in another
    /// project: rows of this project whose successor is one of its activities.
    pub fn external_links(&self) -> Vec<ExternalLink> {
        let pid = self.model.project.proj_id;
        let Some(t) = self.document.table("TASKPRED") else {
            return vec![];
        };
        let own: std::collections::HashSet<i64> =
            self.model.activities.iter().map(|a| a.task_id).collect();
        t.rows
            .iter()
            .filter_map(|r| {
                let link = ExternalLink {
                    row_id: num(t.value(r, "task_pred_id")),
                    pred_project: num(t.value(r, "pred_proj_id")),
                    pred_task: num(t.value(r, "pred_task_id")),
                    succ_project: num(t.value(r, "proj_id")),
                    succ_task: num(t.value(r, "task_id")),
                    link_type: link_type(t.value(r, "pred_type")),
                    lag: parse_hours(t.value(r, "lag_hr_cnt"))
                        .ok()
                        .flatten()
                        .unwrap_or(0),
                };
                (link.succ_project == pid
                    && link.pred_project != pid
                    && own.contains(&link.succ_task))
                .then_some(link)
            })
            .collect()
    }
    /// Add a relationship from activity `pred_task` of project `pred_project`
    /// to this project's activity `succ_task`. Returns its row id.
    pub fn add_external_link(
        &mut self,
        pred_project: i64,
        pred_task: i64,
        succ_task: i64,
        kind: LinkType,
        lag: i64,
    ) -> Result<i64, String> {
        let pid = self.model.project.proj_id;
        if pred_project == pid {
            return Err("Both activities are in this project".into());
        }
        if !self.model.activities.iter().any(|a| a.task_id == succ_task) {
            return Err("The successor is not in this project".into());
        }
        if kind == LinkType::None {
            return Err("Choose a relationship type".into());
        }
        if self.external_links().iter().any(|l| {
            l.pred_project == pred_project && l.pred_task == pred_task && l.succ_task == succ_task
        }) {
            return Err("These activities already have a relationship".into());
        }
        let table = self.document.table_mut("TASKPRED");
        let id = table
            .rows
            .iter()
            .map(|r| num(table.value(r, "task_pred_id")))
            .max()
            .unwrap_or(0)
            + 1;
        table.add(&[
            ("task_pred_id", id.to_string()),
            ("task_id", succ_task.to_string()),
            ("pred_task_id", pred_task.to_string()),
            ("proj_id", pid.to_string()),
            ("pred_proj_id", pred_project.to_string()),
            ("pred_type", kind.as_str().into()),
            ("lag_hr_cnt", out_hours(Some(lag))),
        ]);
        Ok(id)
    }
    /// Change the type and lag of an external relationship this project owns.
    pub fn set_external_link(
        &mut self,
        row_id: i64,
        kind: LinkType,
        lag: i64,
    ) -> Result<(), String> {
        if kind == LinkType::None {
            return Err("Choose a relationship type".into());
        }
        let row = self.external_row(row_id)?;
        let table = self.document.table_mut("TASKPRED");
        if link_type(table.value(&table.rows[row], "pred_type")) != kind
            || table.value(&table.rows[row], "pred_type").is_empty()
        {
            table.set(row, "pred_type", kind.as_str().into());
        }
        if parse_hours(table.value(&table.rows[row], "lag_hr_cnt"))
            .ok()
            .flatten()
            .unwrap_or(0)
            != lag
        {
            table.set(row, "lag_hr_cnt", out_hours(Some(lag)));
        }
        Ok(())
    }
    /// Delete an external relationship this project owns.
    pub fn remove_external_link(&mut self, row_id: i64) -> Result<(), String> {
        let row = self.external_row(row_id)?;
        self.document.table_mut("TASKPRED").rows.remove(row);
        Ok(())
    }
    fn external_row(&self, row_id: i64) -> Result<usize, String> {
        if !self.external_links().iter().any(|l| l.row_id == row_id) {
            return Err("That relationship is not an external relationship of this project".into());
        }
        let t = self.document.table("TASKPRED").ok_or("No relationships")?;
        let pid = self.model.project.proj_id;
        t.rows
            .iter()
            .position(|r| {
                num(t.value(r, "task_pred_id")) == row_id && num(t.value(r, "proj_id")) == pid
            })
            .ok_or_else(|| "Relationship not found".into())
    }
    /// Re-evaluate imported assignment-driven durations against current edits.
    /// The caller schedules a cloned project so an error cannot change live inputs.
    /// Assignments are rebuilt from the current TASKRSRC rows first, so
    /// assignment edits are scheduled (see `rebuild_assignments`).
    pub fn refresh_derived_durations(&mut self) {
        self.rebuild_assignments();
        let current = self.updated_document();
        preprocess_units(&current, &mut self.model, &mut self.details, true);
    }
    pub fn name(&self, index: usize) -> &str {
        self.details
            .get(&self.model.activities[index].task_id)
            .map_or("", |d| d.name.as_str())
    }
    pub fn wbs_label(&self, id: i64) -> String {
        self.wbs.iter().find(|w| w.id == id).map_or_else(
            || "Unassigned".into(),
            |w| format!("{}  {}", w.code, w.name),
        )
    }
    /// Add an activity. Its ID is above every activity ID in the file, including
    /// other projects, because the reference keeps activity IDs unique across a database.
    pub fn add_activity(&mut self, wbs: i64) -> usize {
        self.ensure_a_calendar();
        let in_file = self.document.table("TASK").map_or(0, |t| {
            t.rows
                .iter()
                .map(|r| num(t.value(r, "task_id")))
                .max()
                .unwrap_or(0)
        });
        let id = self
            .model
            .activities
            .iter()
            .map(|a| a.task_id)
            .max()
            .unwrap_or(1000)
            .max(in_file)
            + 10;
        let mut code = format!("A{id}");
        let mut n = 1;
        while self.model.activities.iter().any(|a| a.task_code == code) {
            code = format!("A{id}-{n}");
            n += 1;
        }
        self.details.insert(
            id,
            ActivityDetails {
                name: "New activity".into(),
                wbs,
                original_duration: 480,
                resource_driven_duration: false,
                percent: 0.0,
                minted: Some(self.mint(id)),
            },
        );
        self.model.activities.push(Activity {
            task_id: id,
            task_code: code,
            rem: 480,
            cal: self.model.project_calendar,
            ..Activity::default()
        });
        let i = self.model.activities.len() - 1;
        self.date_unscheduled(&[i]);
        i
    }
    /// A project that opened without any calendar (it had no activities)
    /// gets the standard calendar as its project calendar before its first
    /// activity, so the activity has a calendar to work on and to save.
    /// Everything that makes activities ([`Self::add_activity`], paste)
    /// calls it first.
    pub fn ensure_a_calendar(&mut self) {
        if !self.model.calendars.is_empty() {
            return;
        }
        let id = edit::records::next_id(&self.document, "CALENDAR", "clndr_id").to_string();
        let pid = self.model.project.proj_id;
        let data = standard_calendar_data();
        let Ok(cal) = calendar(&id, &data, self.model.origin_serial) else {
            return;
        };
        edit::records::add_row(
            &mut self.document,
            "CALENDAR",
            &[
                ("clndr_id", id.clone()),
                ("default_flag", "N".into()),
                ("clndr_name", STANDARD_CALENDAR_NAME.into()),
                ("proj_id", pid.to_string()),
                ("clndr_type", "CA_Project".into()),
                ("clndr_data", data),
            ],
        );
        let project = self.document.table_mut("PROJECT");
        if let Some(row) = project
            .rows
            .iter()
            .position(|r| num(project.value(r, "proj_id")) == pid)
        {
            project.set(row, "clndr_id", id);
        }
        self.model.calendars.push(cal);
        self.calendar_names.push(STANDARD_CALENDAR_NAME.into());
        self.model.project_calendar = 0;
    }
    /// A GUID and create date for new activity `task_id` of this project,
    /// minted once when the activity is made and kept in its details.
    pub fn mint(&self, task_id: i64) -> Minted {
        let pid = self.model.project.proj_id;
        let project_guid = self
            .document
            .table("PROJECT")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| num(t.value(r, "proj_id")) == pid)
                    .map(|r| t.value(r, "guid").to_string())
            })
            .unwrap_or_default();
        Minted {
            guid: new_guid(&format!("{project_guid}/{task_id}")),
            date: now_stamp(),
        }
    }
    /// Give activities that have no dates yet (a new activity, a pasted
    /// one) the early dates the engine gives an activity without logic: it
    /// starts at the data date on its own calendar and finishes its
    /// remaining duration later. Only those activities are dated; the rest
    /// of the project is not scheduled, and the new ones carry no late
    /// dates or float until the next schedule run.
    pub fn date_unscheduled(&mut self, indices: &[usize]) {
        let m = &self.model;
        let picked: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| {
                m.activities.get(i).is_some_and(|a| {
                    a.status == StatusType::NotStarted
                        && a.late_start.is_none()
                        && a.total_float.is_none()
                })
            })
            .collect();
        if picked.is_empty() || m.project.data_date.is_none() {
            return;
        }
        let mut alone = Model {
            origin_serial: m.origin_serial,
            project_calendar: m.project_calendar,
            calendars: m.calendars.clone(),
            activities: picked
                .iter()
                .map(|&i| {
                    let mut a = m.activities[i].clone();
                    a.preds.clear();
                    a.succs.clear();
                    a
                })
                .collect(),
            links: vec![],
            project: m.project.clone(),
            options: m.options.clone(),
        };
        alone.options.do_level = false;
        if openxer_engine::scheduler::schedule(&mut alone).is_err() {
            return;
        }
        for (&i, dated) in picked.iter().zip(&alone.activities) {
            let a = &mut self.model.activities[i];
            a.early_start = dated.early_start;
            a.early_finish = dated.early_finish;
        }
    }
    /// The retained document with the model written back. Columns OpenXER does
    /// not own are untouched, and owned cells keep their original text unless the
    /// value they import as has changed.
    pub fn updated_document(&self) -> Document {
        let mut doc = self.document.clone();
        doc.warnings.clear();
        let m = &self.model;
        let pid = m.project.proj_id;
        let origin = m.origin_serial;
        let relative = |s: &str| parse_absolute(s).map(|v| v - origin * DAY);
        let date = move |v: Option<i64>| move || date_string(origin, v);
        let same_date = move |v: Option<i64>| {
            move |old: &str| {
                if old.is_empty() {
                    v.is_none()
                } else {
                    relative(old) == v
                }
            }
        };
        if let Some(project) = doc
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case("PROJECT"))
        {
            if let Some(row) = project
                .rows
                .iter()
                .position(|r| num(project.value(r, "proj_id")) == pid)
            {
                let p = &m.project;
                let mut cols = project.field_index();
                let mut w = RowWriter {
                    table: project,
                    cols: &mut cols,
                    row,
                    fresh: false,
                };
                w.put("proj_short_name", |o| o == p.name, || p.name.clone());
                w.put(
                    "plan_start_date",
                    same_date(p.plan_start),
                    date(p.plan_start),
                );
                w.put("plan_end_date", same_date(p.plan_end), date(p.plan_end));
                w.put("scd_end_date", same_date(p.scd_end), date(p.scd_end));
                w.put(
                    "last_schedule_date",
                    same_date(p.last_schedule),
                    date(p.last_schedule),
                );
                // The data date imports from last_recalc_date, else plan_start_date.
                let imported = relative(w.old("last_recalc_date"))
                    .or_else(|| relative(w.old("plan_start_date")));
                w.put(
                    "last_recalc_date",
                    |_| imported == p.data_date,
                    date(p.data_date),
                );
            }
        }
        self.write_tasks(&mut doc);
        self.write_relationships(&mut doc);
        self.write_options(&mut doc);
        doc
    }
    fn write_tasks(&self, doc: &mut Document) {
        let m = &self.model;
        let pid = m.project.proj_id;
        let origin = m.origin_serial;
        if m.activities.is_empty() && doc.table("TASK").is_none() {
            return;
        }
        let relative = |s: &str| parse_absolute(s).map(|v| v - origin * DAY);
        let mut calendar_ids: HashMap<&str, usize> = HashMap::new();
        for (i, c) in m.calendars.iter().enumerate() {
            calendar_ids.entry(c.id.as_str()).or_insert(i);
        }
        let project_guid = doc
            .table("PROJECT")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| num(t.value(r, "proj_id")) == pid)
                    .map(|r| t.value(r, "guid").to_string())
            })
            .unwrap_or_default();
        let stamp = now_stamp();
        let table = doc.table_mut("TASK");
        let mut cols = table.field_index();
        let proj_c = cols.col("proj_id");
        let task_c = cols.col("task_id");
        let type_c = cols.col("task_type");
        let mut rows: HashMap<i64, usize> = HashMap::with_capacity(m.activities.len());
        for (i, r) in table.rows.iter().enumerate() {
            if num(FieldIndex::get(r, proj_c)) == pid {
                rows.entry(num(FieldIndex::get(r, task_c))).or_insert(i);
            }
        }
        for a in &m.activities {
            let (row, fresh) = match rows.get(&a.task_id) {
                Some(&i) => (i, FieldIndex::get(&table.rows[i], type_c).is_empty()),
                None => {
                    table.rows.push(vec![String::new(); table.fields.len()]);
                    (table.rows.len() - 1, true)
                }
            };
            let d = self.details.get(&a.task_id).cloned().unwrap_or_default();
            let mut w = RowWriter {
                table: &mut *table,
                cols: &mut cols,
                row,
                fresh,
            };
            let date = |v: Option<i64>| move || date_string(origin, v);
            let same_date = |v: Option<i64>| {
                move |old: &str| {
                    if old.is_empty() {
                        v.is_none()
                    } else {
                        relative(old) == v
                    }
                }
            };
            let id = a.task_id.to_string();
            w.put("task_id", |o| num(o) == a.task_id, || id.clone());
            w.put("proj_id", |o| num(o) == pid, || pid.to_string());
            w.put("wbs_id", |o| num(o) == d.wbs, || d.wbs.to_string());
            w.put(
                "clndr_id",
                |o| calendar_ids.get(o).copied().unwrap_or(m.project_calendar) == a.cal,
                || {
                    m.calendars
                        .get(a.cal)
                        .map_or_else(String::new, |c| c.id.clone())
                },
            );
            w.put(
                "phys_complete_pct",
                |o| o.parse().unwrap_or(0.0) == d.percent,
                || d.percent.to_string(),
            );
            w.put(
                "task_type",
                |o| task_type(o) == Some(a.task_type),
                || a.task_type.as_str().into(),
            );
            w.put(
                "status_code",
                |o| status(o) == a.status,
                || {
                    match a.status {
                        StatusType::NotStarted => "TK_NotStart",
                        StatusType::InProgress => "TK_Active",
                        StatusType::Completed => "TK_Complete",
                    }
                    .into()
                },
            );
            w.put("task_code", |o| o == a.task_code, || a.task_code.clone());
            w.put("task_name", |o| o == d.name, || d.name.clone());
            w.put(
                "total_float_hr_cnt",
                |o| optional_hours(o) == a.total_float,
                || out_hours(a.total_float),
            );
            w.put(
                "free_float_hr_cnt",
                |o| optional_hours(o) == a.free_float,
                || out_hours(a.free_float),
            );
            w.put(
                "remain_drtn_hr_cnt",
                |o| hours(o) == a.rem,
                || out_hours(Some(a.rem)),
            );
            w.put(
                "target_drtn_hr_cnt",
                |o| hours(o) == d.original_duration,
                || out_hours(Some(d.original_duration)),
            );
            for (k, v) in [
                ("cstr_date", a.cstr_date),
                ("act_start_date", a.act_start),
                ("act_end_date", a.act_finish),
                ("late_start_date", a.late_start),
                ("late_end_date", a.late_finish),
                ("expect_end_date", a.expect_end),
                ("early_start_date", a.early_start),
                ("early_end_date", a.early_finish),
                ("restart_date", a.restart),
                ("reend_date", a.reend),
                ("target_start_date", a.target_start),
                ("target_end_date", a.target_finish),
                ("rem_late_start_date", a.rem_late_start),
                ("rem_late_end_date", a.rem_late_finish),
                ("resume_date", a.resume),
                ("external_early_start_date", a.ext_early_start),
                ("external_late_end_date", a.ext_late_end),
            ] {
                w.put(k, same_date(v), date(v));
            }
            w.put(
                "cstr_type",
                |o| constraint(o) == a.cstr,
                || constraint_code(a.cstr).into(),
            );
            // A new activity outside any float path carries blanks, as the reference
            // writes them.
            w.put(
                "float_path",
                |o| num(o) == a.float_path,
                || match (fresh, a.float_path) {
                    (true, 0) => String::new(),
                    (_, n) => n.to_string(),
                },
            );
            w.put(
                "float_path_order",
                |o| num(o) == a.float_path_order,
                || match (fresh, a.float_path_order) {
                    (true, 0) => String::new(),
                    (_, n) => n.to_string(),
                },
            );
            // The secondary constraint imports only beside a primary one.
            let primary = !w.old("cstr_type").is_empty();
            w.put(
                "cstr_date2",
                |o| primary.then(|| relative(o)).flatten() == a.cstr_date2,
                date(a.cstr_date2),
            );
            w.put(
                "cstr_type2",
                |o| {
                    (if primary {
                        constraint(o)
                    } else {
                        CstrType::None
                    }) == a.cstr2
                },
                || constraint_code(a.cstr2).into(),
            );
            w.put(
                "driving_path_flag",
                |o| (o == "Y") == a.driving_path,
                || yes_no(a.driving_path),
            );
            if fresh {
                // The reference gives new milestones no estimate weight.
                if matches!(a.task_type, TaskType::Mile | TaskType::FinMile) {
                    w.default("est_wt", "0");
                }
                for (k, v) in NEW_TASK_DEFAULTS {
                    w.default(k, v);
                }
                // The reference always carries planned dates; an unscheduled new activity
                // starts and finishes at the data date until it is scheduled.
                let start = a.target_start.or(a.early_start).or(m.project.data_date);
                let finish = a.target_finish.or(a.early_finish).or(m.project.data_date);
                w.default("target_start_date", &date_string(origin, start));
                w.default("target_end_date", &date_string(origin, finish));
                // Minted when the activity was made, so every export agrees; an
                // activity made some other way gets new values each time.
                let minted = d.minted.clone().unwrap_or_else(|| Minted {
                    guid: new_guid(&format!("{project_guid}/{}", a.task_id)),
                    date: stamp.clone(),
                });
                w.default("guid", &minted.guid);
                w.default("create_date", &minted.date);
                w.default("update_date", &minted.date);
            }
        }
    }
    fn write_relationships(&self, doc: &mut Document) {
        let m = &self.model;
        let pid = m.project.proj_id;
        if m.links.is_empty() && doc.table("TASKPRED").is_none() {
            return;
        }
        // Activities known to this project, in the file or the model. A row whose
        // endpoint is neither was never imported and stays as it is.
        let mut known: std::collections::HashSet<i64> =
            m.activities.iter().map(|a| a.task_id).collect();
        if let Some(t) = doc.table("TASK") {
            known.extend(
                t.rows
                    .iter()
                    .filter(|r| num(t.value(r, "proj_id")) == pid)
                    .map(|r| num(t.value(r, "task_id"))),
            );
        }
        let table = doc.table_mut("TASKPRED");
        let mut cols = table.field_index();
        let c_pred = cols.col("pred_task_id");
        let c_task = cols.col("task_id");
        let c_proj = cols.col("proj_id");
        let c_pred_proj = cols.col("pred_proj_id");
        let c_id = cols.col("task_pred_id");
        let mut c_type = cols.col("pred_type");
        let mut candidates: HashMap<(i64, i64), Vec<usize>> = HashMap::with_capacity(m.links.len());
        for (i, r) in table.rows.iter().enumerate() {
            // A relationship with another project is never taken for one of
            // this project's own, even when the activity ids match.
            let pred_project = FieldIndex::get(r, c_pred_proj).trim();
            if num(FieldIndex::get(r, c_proj)) == pid
                && (pred_project.is_empty() || num(pred_project) == pid)
            {
                let key = (
                    num(FieldIndex::get(r, c_pred)),
                    num(FieldIndex::get(r, c_task)),
                );
                candidates.entry(key).or_default().push(i);
            }
        }
        let link_types: std::collections::HashSet<(i64, i64, &str)> = m
            .links
            .iter()
            .map(|l| {
                (
                    m.activities[l.pred].task_id,
                    m.activities[l.succ].task_id,
                    l.link_type.as_str(),
                )
            })
            .collect();
        let mut used = vec![false; table.rows.len()];
        let mut next = table
            .rows
            .iter()
            .map(|r| num(FieldIndex::get(r, c_id)))
            .max()
            .unwrap_or(0)
            + 1;
        for l in &m.links {
            let pred = m.activities[l.pred].task_id;
            let succ = m.activities[l.succ].task_id;
            let rows = candidates.get(&(pred, succ)).map_or(&[][..], Vec::as_slice);
            // A row whose stored type no link of this pair uses can take this link.
            let free_type = |i: usize| {
                let raw = FieldIndex::get(&table.rows[i], c_type);
                [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF]
                    .into_iter()
                    .find(|t| t.as_str() == raw)
                    .is_none_or(|t| !link_types.contains(&(pred, succ, t.as_str())))
            };
            let same_type = rows.iter().copied().find(|&i| {
                !used[i] && FieldIndex::get(&table.rows[i], c_type) == l.link_type.as_str()
            });
            let (row, fresh) = match same_type
                .or_else(|| rows.iter().copied().find(|&i| !used[i] && free_type(i)))
            {
                Some(i) => (i, false),
                None => {
                    table.rows.push(vec![String::new(); table.fields.len()]);
                    used.push(false);
                    let i = table.rows.len() - 1;
                    cols.set(table, i, "task_pred_id", next.to_string());
                    next += 1;
                    (i, true)
                }
            };
            used[row] = true;
            let mut w = RowWriter {
                table: &mut *table,
                cols: &mut cols,
                row,
                fresh,
            };
            w.put("task_id", |o| num(o) == succ, || succ.to_string());
            w.put("pred_task_id", |o| num(o) == pred, || pred.to_string());
            w.put("proj_id", |o| num(o) == pid, || pid.to_string());
            w.put("pred_proj_id", |o| num(o) == pid, || pid.to_string());
            w.put(
                "pred_type",
                |o| o == l.link_type.as_str(),
                || l.link_type.as_str().into(),
            );
            w.put(
                "lag_hr_cnt",
                |o| hours(o) == l.lag,
                || out_hours(Some(l.lag)),
            );
            c_type = cols.col("pred_type");
        }
        let columns = ["proj_id", "pred_proj_id", "pred_task_id", "task_id"].map(|k| cols.col(k));
        let mut n = 0;
        table.rows.retain(|r| {
            let [proj, pred_proj, pred, succ] =
                columns.map(|c| c.and_then(|c| r.get(c)).map_or(0, |v| num(v)));
            // Rows of this project that imported as links and are no longer used.
            let removed = proj == pid
                && pred_proj == pid
                && known.contains(&pred)
                && known.contains(&succ)
                && !used[n];
            n += 1;
            !removed
        });
    }
    fn write_options(&self, doc: &mut Document) {
        let m = &self.model;
        let pid = m.project.proj_id;
        let o = &m.options;
        let existing = doc.table("SCHEDOPTIONS").and_then(|t| {
            t.rows
                .iter()
                .position(|r| num(t.value(r, "proj_id")) == pid)
        });
        if existing.is_none()
            && *o
                == (ScheduleOptions {
                    do_level: o.do_level,
                    ..default_options()
                })
        {
            return;
        }
        let table = doc.table_mut("SCHEDOPTIONS");
        let (row, fresh) = match existing {
            Some(i) => (i, false),
            None => {
                let id = table
                    .rows
                    .iter()
                    .map(|r| num(table.value(r, "schedoptions_id")))
                    .max()
                    .unwrap_or(0)
                    + 1;
                table.rows.push(vec![String::new(); table.fields.len()]);
                let i = table.rows.len() - 1;
                table.set(i, "schedoptions_id", id.to_string());
                (i, true)
            }
        };
        let mut cols = table.field_index();
        let mut w = RowWriter {
            table,
            cols: &mut cols,
            row,
            fresh,
        };
        w.put("proj_id", |v| num(v) == pid, || pid.to_string());
        for (k, value, default) in [
            ("sched_retained_logic", o.retained_logic, true),
            ("sched_progress_override", o.progress_override, false),
            ("sched_lag_early_start_flag", o.lag_from_early_start, true),
            ("sched_use_expect_end_flag", o.use_expected_finish, true),
            ("sched_open_critical_flag", o.open_end_uses_early, false),
            (
                "sched_use_project_end_date_for_float",
                o.use_must_finish,
                true,
            ),
        ] {
            w.put(k, |v| flag_or(v, default) == value, || yes_no(value));
        }
        w.put(
            "sched_outer_depend_type",
            |v| out_of_sequence(v) == o.out_of_sequence,
            || {
                if o.out_of_sequence == OutOfSequence::None {
                    "SD_None"
                } else {
                    "SD_Both"
                }
                .into()
            },
        );
        w.put(
            "sched_float_type",
            |v| float_type(v) == o.float_type,
            || o.float_type.as_str().into(),
        );
        w.put(
            "enable_multiple_longest_path_calc",
            |v| (v == "Y") == o.multiple_float_paths,
            || yes_no(o.multiple_float_paths),
        );
        w.put(
            "use_total_float_multiple_longest_paths",
            |v| (v != "N") == o.float_paths_use_total,
            || yes_no(o.float_paths_use_total),
        );
        w.put(
            "max_multiple_longest_path",
            |v| max_paths(v) == o.max_float_paths,
            || o.max_float_paths.to_string(),
        );
        w.put(
            "key_activity_for_multiple_longest_paths",
            // An ID the project does not hold imported as blank, so it is
            // compared as blank and kept until the option is changed.
            |v| {
                key_activity(v).filter(|id| m.activities.iter().any(|a| a.task_id == *id))
                    == o.float_paths_end_activity
            },
            || {
                o.float_paths_end_activity
                    .map_or(String::new(), |id| id.to_string())
            },
        );
        w.put(
            "sched_calendar_on_relationship_lag",
            |v| lag_calendar(v) == o.lag_calendar,
            || {
                match o.lag_calendar {
                    LagCalendar::PredCal => "rcal_Predecessor",
                    LagCalendar::SuccCal => "rcal_Successor",
                    LagCalendar::Hour24 => "rcal_24Hour",
                    LagCalendar::ProjCal => "rcal_ProjDefault",
                }
                .into()
            },
        );
        if fresh {
            for (k, v) in [
                ("sched_setplantoforecast", "N"),
                ("level_float_thrs_cnt", "0"),
                ("level_outer_assign_flag", "N"),
                ("level_outer_assign_priority", "5"),
                ("level_over_alloc_pct", "0"),
                ("level_within_float_flag", "N"),
                ("level_keep_sched_date_flag", "Y"),
                ("level_all_rsrc_flag", "Y"),
                ("limit_multiple_longest_path_calc", "Y"),
            ] {
                w.default(k, v);
            }
        }
    }
}
/// A WBS element whose parent chain loops is shown at the top level.
fn break_wbs_cycles(wbs: &mut [Wbs], warnings: &mut Vec<String>) {
    let index: HashMap<i64, usize> = wbs.iter().enumerate().map(|(i, w)| (w.id, i)).collect();
    if index.len() != wbs.len() {
        warnings.push("Some WBS IDs appear more than once; the last row of each is used.".into());
    }
    for start in 0..wbs.len() {
        let mut seen = std::collections::HashSet::new();
        let mut current = wbs[start].parent;
        while let Some(id) = current {
            if id == wbs[start].id {
                warnings.push(format!(
                    "WBS {} has a circular parent chain; it is shown at the top level.",
                    wbs[start].code
                ));
                wbs[start].parent = None;
                break;
            }
            if !seen.insert(id) {
                break;
            }
            current = index.get(&id).and_then(|&i| wbs[i].parent);
        }
    }
}

/// Name of the calendar new projects start with.
pub const STANDARD_CALENDAR_NAME: &str = "Standard 5-day / 8-hour";
/// `clndr_data` of the calendar new projects start with: Monday to Friday,
/// 08:00 to 12:00 and 13:00 to 17:00.
pub fn standard_calendar_data() -> String {
    let week = (1..=7)
        .map(|d| {
            if (2..=6).contains(&d) {
                format!("(0||{d}()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))")
            } else {
                format!("(0||{d}()())")
            }
        })
        .collect::<String>();
    format!("(0||CalendarData()((0||DaysOfWeek()({week}))(0||Exceptions()())))")
}
/// A stored calendar (`clndr_data` text) as the engine's calendar, with
/// minutes counted from day `origin` (a serial day number).
pub fn calendar_from_data(id: &str, data: &str, origin: i64) -> Result<CalendarAdapter, String> {
    calendar(id, data, origin)
}
pub fn starter() -> ProjectData {
    let mut doc = Document {
        header: "ERMHDR\t25.12\t2026-11-04\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD".into(),
        tables: vec![],
        windows_encoding: true,
        warnings: vec![],
        lone_quotes: BTreeMap::new(),
        id_floor: BTreeMap::new(),
    };
    doc.table_mut("PROJECT").add(&[
        ("proj_id", "1".into()),
        ("proj_short_name", "RIVERBEND".into()),
        ("last_recalc_date", "2026-11-02 08:00".into()),
        ("plan_start_date", "2026-11-02 08:00".into()),
        ("clndr_id", "1".into()),
        ("rem_target_link_flag", "Y".into()),
    ]);
    doc.table_mut("CALENDAR").add(&[
        ("clndr_id", "1".into()),
        ("clndr_name", STANDARD_CALENDAR_NAME.into()),
        ("clndr_data", standard_calendar_data()),
    ]);
    for (id, code, name) in [
        (10, "RB.1", "Engineering & approvals"),
        (20, "RB.2", "Procurement"),
        (30, "RB.3", "Construction"),
        (40, "RB.4", "Commissioning & turnover"),
    ] {
        doc.table_mut("PROJWBS").add(&[
            ("wbs_id", id.to_string()),
            ("proj_id", "1".into()),
            ("wbs_short_name", code.into()),
            ("wbs_name", name.into()),
            ("seq_num", id.to_string()),
        ]);
    }
    let names = [
        ("Notice to proceed", 10, 0),
        ("Site investigation & survey", 10, 3),
        ("Complete detailed design", 10, 6),
        ("Issue construction package", 10, 0),
        ("Purchase long-lead equipment", 20, 8),
        ("Fabricate structural assemblies", 20, 6),
        ("Mobilize site team", 30, 2),
        ("Prepare foundations", 30, 5),
        ("Install structural assemblies", 30, 4),
        ("Mechanical installation", 30, 6),
        ("Electrical & instrumentation", 30, 5),
        ("Pre-commissioning checks", 40, 3),
        ("System commissioning", 40, 3),
        ("Final inspection & handover", 40, 0),
    ];
    for (i, (name, wbs, days)) in names.iter().enumerate() {
        doc.table_mut("TASK").add(&[
            ("task_id", (1000 + i * 10).to_string()),
            ("proj_id", "1".into()),
            ("wbs_id", wbs.to_string()),
            ("clndr_id", "1".into()),
            ("task_code", format!("RB-{:04}", 1000 + i * 10)),
            ("task_name", name.to_string()),
            (
                "task_type",
                if *days == 0 {
                    if i == 0 {
                        "TT_Mile"
                    } else {
                        "TT_FinMile"
                    }
                } else {
                    "TT_Task"
                }
                .into(),
            ),
            ("status_code", "TK_NotStart".into()),
            ("duration_type", "DT_FixedDUR2".into()),
            ("remain_drtn_hr_cnt", (days * 8).to_string()),
            ("target_drtn_hr_cnt", (days * 8).to_string()),
        ]);
    }
    for (i, (p, s)) in [
        (0, 1),
        (1, 2),
        (2, 3),
        (2, 4),
        (3, 5),
        (3, 6),
        (6, 7),
        (5, 8),
        (7, 8),
        (4, 9),
        (8, 9),
        (8, 10),
        (9, 11),
        (10, 11),
        (11, 12),
        (12, 13),
    ]
    .iter()
    .enumerate()
    {
        doc.table_mut("TASKPRED").add(&[
            ("task_pred_id", (i + 1).to_string()),
            ("proj_id", "1".into()),
            ("pred_proj_id", "1".into()),
            ("pred_task_id", (1000 + p * 10).to_string()),
            ("task_id", (1000 + s * 10).to_string()),
            ("pred_type", "PR_FS".into()),
            ("lag_hr_cnt", "0".into()),
        ]);
    }
    ProjectData::from_document(doc, 1, "Starter project".into()).expect("built-in starter is valid")
}
/// Two starter projects in one file, `RIVERBEND` (project 1) and `NORTHYARD`
/// (project 2), joined by one cross-project relationship: RIVERBEND's
/// "Issue construction package" (1030) finish-to-start NORTHYARD's notice to
/// proceed (2000). Sample data for multi-project tests and demonstrations.
pub fn starter_portfolio() -> Document {
    let mut doc = starter().document;
    let copy = |doc: &mut Document, name: &str, edit: &dyn Fn(&mut Table, &mut Vec<String>)| {
        let table = doc.table_mut(name);
        let rows: Vec<Vec<String>> = table
            .rows
            .iter()
            .filter(|r| table.value(r, "proj_id") == "1")
            .cloned()
            .collect();
        for mut r in rows {
            edit(table, &mut r);
            table.rows.push(r);
        }
    };
    let shift = |t: &Table, r: &mut [String], field: &str, by: i64| {
        if let Some(c) = t.field(field) {
            if !r[c].is_empty() {
                r[c] = (num(&r[c]) + by).to_string();
            }
        }
    };
    let set = |t: &Table, r: &mut [String], field: &str, value: &str| {
        if let Some(c) = t.field(field) {
            r[c] = value.into();
        }
    };
    copy(&mut doc, "PROJECT", &|t, r| {
        set(t, r, "proj_id", "2");
        set(t, r, "proj_short_name", "NORTHYARD");
    });
    copy(&mut doc, "PROJWBS", &|t, r| {
        set(t, r, "proj_id", "2");
        shift(t, r, "wbs_id", 100);
        let code = t.value(r, "wbs_short_name").replace("RB", "NY");
        set(t, r, "wbs_short_name", &code);
    });
    copy(&mut doc, "TASK", &|t, r| {
        set(t, r, "proj_id", "2");
        shift(t, r, "task_id", 1000);
        shift(t, r, "wbs_id", 100);
        let code = t.value(r, "task_code").replace("RB-1", "NY-2");
        set(t, r, "task_code", &code);
    });
    copy(&mut doc, "TASKPRED", &|t, r| {
        set(t, r, "proj_id", "2");
        set(t, r, "pred_proj_id", "2");
        shift(t, r, "task_pred_id", 100);
        shift(t, r, "task_id", 1000);
        shift(t, r, "pred_task_id", 1000);
    });
    doc.table_mut("TASKPRED").add(&[
        ("task_pred_id", "200".into()),
        ("task_id", "2000".into()),
        ("pred_task_id", "1030".into()),
        ("proj_id", "2".into()),
        ("pred_proj_id", "1".into()),
        ("pred_type", "PR_FS".into()),
        ("lag_hr_cnt", "0".into()),
    ]);
    doc
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("openxer-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.xer");
        std::fs::write(&path, "old").unwrap();
        write_file_atomic(&path, b"new contents").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new contents");
        // A write that cannot finish leaves the old file and no temporary.
        let blocked = dir.join("blocked");
        std::fs::create_dir_all(blocked.join("inside")).unwrap();
        assert!(write_file_atomic(&blocked, b"x").is_err());
        assert!(blocked.join("inside").is_dir());
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(left.iter().all(|n| !n.ends_with(".tmp")), "{left:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn portfolio_projects_and_external_links() {
        let doc = starter_portfolio();
        assert_eq!(doc.projects().len(), 2);
        let mut ny = ProjectData::from_document(doc.clone(), 2, "p".into()).unwrap();
        let rb = ProjectData::from_document(doc, 1, "p".into()).unwrap();
        assert_eq!((ny.model.activities.len(), ny.model.links.len()), (14, 16));
        assert!(rb.external_links().is_empty());
        let links = ny.external_links();
        assert_eq!(links.len(), 1);
        assert_eq!(
            (
                links[0].pred_project,
                links[0].pred_task,
                links[0].succ_task
            ),
            (1, 1030, 2000)
        );
        ny.set_external_link(200, LinkType::SS, 480).unwrap();
        assert_eq!(ny.external_links()[0].link_type, LinkType::SS);
        assert_eq!(ny.external_links()[0].lag, 480);
        let id = ny
            .add_external_link(1, 1130, 2130, LinkType::FF, 0)
            .unwrap();
        assert!(ny
            .add_external_link(1, 1130, 2130, LinkType::FS, 0)
            .is_err());
        assert!(ny
            .add_external_link(2, 2000, 2010, LinkType::FS, 0)
            .is_err());
        assert_eq!(ny.external_links().len(), 2);
        // The edits survive export and import.
        let back = ProjectData::from_document(
            Document::parse(&ny.updated_document().text()).unwrap(),
            2,
            "p".into(),
        )
        .unwrap();
        assert_eq!(back.external_links(), ny.external_links());
        ny.remove_external_link(id).unwrap();
        ny.remove_external_link(200).unwrap();
        assert!(ny.external_links().is_empty());
        assert!(ny.remove_external_link(200).is_err());
    }
    #[test]
    fn external_dates_are_written_and_kept() {
        let mut p = starter();
        let before = p.updated_document().text();
        assert_eq!(
            ProjectData::from_document(Document::parse(&before).unwrap(), 1, "s".into())
                .unwrap()
                .updated_document()
                .text(),
            before
        );
        p.model.activities[2].ext_early_start = p.model.project.data_date.map(|d| d + 2 * 1440);
        let doc = Document::parse(&p.updated_document().text()).unwrap();
        let back = ProjectData::from_document(doc, 1, "s".into()).unwrap();
        assert_eq!(
            back.model.activities[2].ext_early_start,
            p.model.activities[2].ext_early_start
        );
        assert_eq!(back.model.activities[2].ext_late_end, None);
    }
    #[test]
    fn starter_round_trip_preserves_all_tables() {
        let p = starter();
        let doc = p.updated_document();
        let q = ProjectData::from_document(Document::parse(&doc.text()).unwrap(), 1, "test".into())
            .unwrap();
        assert_eq!(p.model.activities.len(), q.model.activities.len());
        assert_eq!(p.model.links.len(), q.model.links.len());
        assert_eq!(p.details.len(), q.details.len());
        assert_eq!(q.model.calendars[0].cal.total_hours_on_date(40), 480);
    }
    #[test]
    fn unknown_columns_survive_edit() {
        let mut p = starter();
        p.document
            .table_mut("TASK")
            .set(0, "custom_vendor_field", "keep me".into());
        p.model.activities[0].task_code = "CHANGED".into();
        let out = p.updated_document();
        let t = out.table("TASK").unwrap();
        assert_eq!(t.value(&t.rows[0], "custom_vendor_field"), "keep me");
        assert_eq!(t.value(&t.rows[0], "task_code"), "CHANGED");
    }
    #[test]
    fn parallel_relationship_comments_stay_with_their_type() {
        let mut document = starter().document;
        let table = document.table_mut("TASKPRED");
        table.set(0, "comments", "FS comment".into());
        let mut duplicate = table.rows[0].clone();
        let type_column = table.field("pred_type").unwrap();
        duplicate[type_column] = "PR_SS".into();
        table.rows.push(duplicate);
        let last = table.rows.len() - 1;
        table.set(last, "task_pred_id", "9000".into());
        table.set(last, "comments", "SS comment".into());
        let mut project = ProjectData::from_document(document, 1, "parallel".into()).unwrap();
        let removed = project.model.links.remove(0);
        project.model.rebuild_links().unwrap();
        let output = project.updated_document();
        let table = output.table("TASKPRED").unwrap();
        let row = table
            .rows
            .iter()
            .find(|r| table.value(r, "task_pred_id") == "9000")
            .unwrap();
        assert_eq!(table.value(row, "comments"), "SS comment");
        assert_eq!(table.value(row, "pred_type"), "PR_SS");
        assert!(!table
            .rows
            .iter()
            .any(|r| table.value(r, "comments") == "FS comment"));
        project.model.links.insert(
            0,
            Link {
                link_type: LinkType::FF,
                ..removed
            },
        );
        let output = project.updated_document();
        let table = output.table("TASKPRED").unwrap();
        let fs = table
            .rows
            .iter()
            .find(|r| table.value(r, "comments") == "FS comment")
            .unwrap();
        assert_eq!(table.value(fs, "pred_type"), "PR_FF");
        let ss = table
            .rows
            .iter()
            .find(|r| table.value(r, "task_pred_id") == "9000")
            .unwrap();
        assert_eq!(table.value(ss, "comments"), "SS comment");
    }
    #[test]
    fn calendar_del_line_breaks() {
        let raw = "(0||CalendarData()(\u{7f}\u{7f}(0||DaysOfWeek()((0||2()((0||0(s|08:00|f|16:00)())))))(0||Exceptions()())))";
        let c = calendar("del", raw, 46328).unwrap();
        assert!(c.cal.template.iter().any(|s| s == &vec![(480, 960)]));
    }
    #[test]
    fn unicode_and_dates() {
        let d = parse_absolute("2026-11-04 08:15").unwrap();
        let origin = d.div_euclid(DAY);
        assert_eq!(
            date_string(origin, Some(d - origin * DAY)),
            "2026-11-04 08:15"
        );
        assert!(Document::parse("").is_err());
        assert!(calendar("bad", "not a calendar", origin).is_err());
    }
}

fn leveling_inputs(doc: &Document, m: &Model, wbs: &[Wbs]) -> LevelingInputs {
    let mut out = LevelingInputs {
        horizon: m
            .activities
            .iter()
            .flat_map(|a| {
                [
                    a.early_finish,
                    a.late_finish,
                    a.target_finish,
                    a.expect_end,
                    a.cstr_date,
                    a.cstr_date2,
                ]
            })
            .flatten()
            .max()
            .unwrap_or(m.project.data_date.unwrap_or(0))
            + 4000 * DAY,
        ..Default::default()
    };
    let pid = m.project.proj_id;
    let mut code_types = BTreeMap::new();
    if let Some(table) = doc.table("ACTVTYPE") {
        for row in &table.rows {
            let name = table.value(row, "actv_code_type").to_ascii_uppercase();
            let normalized = name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("_");
            let project = table.value(row, "proj_id");
            let key = if project.is_empty() {
                format!("ACTVCODE_{normalized}")
            } else {
                format!("ACTVCODE_{normalized}_{project}")
            };
            code_types.insert(key, num(table.value(row, "actv_code_type_id")));
        }
    }
    let mut code_values = BTreeMap::new();
    if let Some(table) = doc.table("ACTVCODE") {
        for row in &table.rows {
            // Leveling ranks activity code values by their short name as text
            // ("10" before "2"), not by the code's sequence number: with the
            // short name, 369.xer's 3,126 stored leveled remaining date pairs
            // all match; by sequence, 1,664 do (probe simulator rule, probes 801-813).
            code_values.insert(
                num(table.value(row, "actv_code_id")),
                table.value(row, "short_name").to_string(),
            );
        }
    }
    if let Some(table) = doc.table("TASKACTV") {
        let cols = table.field_index();
        let c_proj = cols.col("proj_id");
        let c_code = cols.col("actv_code_id");
        let c_task = cols.col("task_id");
        let c_type = cols.col("actv_code_type_id");
        for row in &table.rows {
            if num(FieldIndex::get(row, c_proj)) != pid {
                continue;
            }
            if let Some(value) = code_values.get(&num(FieldIndex::get(row, c_code))) {
                out.activity_codes.insert(
                    (
                        num(FieldIndex::get(row, c_task)),
                        num(FieldIndex::get(row, c_type)),
                    ),
                    value.clone(),
                );
            }
        }
    }
    // "Max percent to over-allocate resources" applies only to leveling
    // within total float, which is rejected below. Leveling outside float
    // gives the same dates at 25% and at 0% (controlled test schedules, and
    // a corpus file whose stored leveled dates match only without it).
    // Without "Level all resources", only the resources selected for leveling
    // (RSRCLEVELLIST) are leveled; the others are booked against no limit.
    let mut selected: Option<BTreeSet<i64>> = None;
    if let Some(t) = doc.table("SCHEDOPTIONS") {
        if let Some(r) = t.rows.iter().find(|r| num(t.value(r, "proj_id")) == pid) {
            let v = |k| t.value(r, k);
            if v("level_all_rsrc_flag") == "N" {
                let id = v("schedoptions_id");
                selected = Some(doc.table("RSRCLEVELLIST").map_or_else(BTreeSet::new, |l| {
                    l.rows
                        .iter()
                        .filter(|x| l.value(x, "schedoptions_id") == id)
                        .map(|x| num(l.value(x, "rsrc_id")))
                        .collect()
                }));
            }
            // Leveling only within float keeps `level_float_thrs_cnt` hours of
            // float; the threshold is read only in that mode (LVL-3).
            if v("level_within_float_flag") == "Y" {
                out.within_float = true;
                out.preserve_float = (v("level_float_thrs_cnt")
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|h| h.is_finite() && *h > 0.)
                    .unwrap_or(0.)
                    * 60.)
                    .round() as i64;
            }
            if v("level_keep_sched_date_flag") == "N" {
                out.unsupported
                    .push("Replace scheduled early and late dates during leveling".into());
            }
            if v("level_outer_assign_flag") == "Y" {
                out.unsupported
                    .push("Consider assignments in other projects".into());
            }
            for item in v("LevelPriorityList")
                .split('\u{7f}')
                .filter(|s| !s.is_empty())
            {
                let key = item
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .split('|')
                    .next()
                    .unwrap_or("");
                let field = match key {
                    "priority_type" => Some(PriorityField::ActivityPriority),
                    "total_float_hr_cnt" => Some(PriorityField::TotalFloat),
                    "early_start_date" => Some(PriorityField::EarlyStart),
                    "late_start_date" => Some(PriorityField::LateStart),
                    "remain_drtn_hr_cnt" => Some(PriorityField::RemainingDuration),
                    "wbs_id" => Some(PriorityField::Wbs),
                    _ => code_types
                        .get(key)
                        .copied()
                        .map(PriorityField::ActivityCode),
                };
                if let Some(field) = field {
                    out.priorities.push(LevelPriority {
                        field,
                        ascending: !item.contains("DESC"),
                    });
                } else {
                    out.unsupported.push(format!("Leveling priority {key}"));
                }
            }
        }
    }
    let mut calendar_ids: HashMap<&str, usize> = HashMap::new();
    for (i, c) in m.calendars.iter().enumerate() {
        calendar_ids.entry(c.id.as_str()).or_insert(i);
    }
    let mut resource_calendars = HashMap::new();
    if let Some(t) = doc.table("RSRC") {
        for r in &t.rows {
            let index = calendar_ids.get(t.value(r, "clndr_id")).copied();
            resource_calendars.insert(num(t.value(r, "rsrc_id")), index);
        }
    }
    if let Some(t) = doc.table("RSRCRATE") {
        for r in &t.rows {
            if let Some(date) = parse_absolute(t.value(r, "start_date")) {
                let capacity = t.value(r, "max_qty_per_hr").parse::<f64>().unwrap_or(0.);
                out.limits
                    .entry(num(t.value(r, "rsrc_id")))
                    .or_default()
                    .push((date - m.origin_serial * DAY, capacity));
            }
        }
    }
    for list in out.limits.values_mut() {
        list.sort_by_key(|r| r.0);
    }
    out.selected = selected;
    if let Some(t) = doc.table("TASKRSRC") {
        let cols = t.field_index();
        let c_proj = cols.col("proj_id");
        let c_rsrc = cols.col("rsrc_id");
        let c_rate = cols.col("remain_qty_per_hr");
        let c_qty = cols.col("remain_qty");
        let c_task = cols.col("task_id");
        for r in &t.rows {
            if num(FieldIndex::get(r, c_proj)) != pid {
                continue;
            }
            let resource_id = num(FieldIndex::get(r, c_rsrc));
            let rate = FieldIndex::get(r, c_rate).parse::<f64>().unwrap_or(0.);
            let remaining_units = FieldIndex::get(r, c_qty).parse::<f64>().unwrap_or(0.);
            out.demands
                .entry(num(FieldIndex::get(r, c_task)))
                .or_default()
                .push(ResourceDemand {
                    resource_id,
                    rate,
                    remaining_units,
                    duration: if rate > 0. {
                        (remaining_units / rate * 60.).round_ties_even() as i64
                    } else {
                        0
                    },
                    calendar: resource_calendars.get(&resource_id).copied().flatten(),
                });
        }
    }
    let wbs_index: HashMap<i64, &Wbs> = wbs.iter().map(|w| (w.id, w)).collect();
    if let Some(t) = doc.table("TASK") {
        let cols = t.field_index();
        let c_proj = cols.col("proj_id");
        let c_task = cols.col("task_id");
        let c_prio = cols.col("priority_type");
        let c_type = cols.col("task_type");
        let c_wbs = cols.col("wbs_id");
        for r in &t.rows {
            if num(FieldIndex::get(r, c_proj)) != pid {
                continue;
            }
            let id = num(FieldIndex::get(r, c_task));
            let priority = match FieldIndex::get(r, c_prio) {
                "PT_Top" => 1,
                "PT_High" => 2,
                "PT_Low" => 4,
                "PT_Lowest" => 5,
                _ => 3,
            };
            out.activity_priority.insert(id, priority);
            if FieldIndex::get(r, c_type) == "TT_Rsrc" {
                out.resource_dependent.insert(id);
            }
            let mut path = vec![];
            let mut parent = Some(num(FieldIndex::get(r, c_wbs)));
            let mut seen = std::collections::BTreeSet::new();
            while let Some(key) = parent {
                if !seen.insert(key) {
                    break;
                }
                if let Some(w) = wbs_index.get(&key) {
                    path.push((w.sequence, w.id));
                    parent = w.parent;
                } else {
                    break;
                }
            }
            path.reverse();
            out.wbs_paths.insert(id, path);
        }
    }
    out
}

/// The exact quotient of the two binary floats (as rationals, not rounded
/// first), rounded half to even to a whole minute.
fn units_minutes(qty: f64, rate: f64, lag: f64) -> Option<i64> {
    if !qty.is_finite() || !rate.is_finite() || !lag.is_finite() {
        return None;
    }
    if (qty <= 0. || rate <= 0.) && lag == 0. {
        return Some(0);
    }
    if qty > 0. && rate > 0. {
        let v = (lag + qty / rate) * 60.;
        if v.is_finite() && v.abs() < 1.0e9 && (v.fract().abs() - 0.5).abs() > 1.0e-4 {
            return Some(v.round_ties_even() as i64);
        }
    }
    units_minutes_exact(qty, rate, lag)
}
fn units_minutes_exact(qty: f64, rate: f64, lag: f64) -> Option<i64> {
    use num_bigint::BigInt;
    use num_rational::BigRational;
    use num_traits::{Signed, ToPrimitive, Zero};
    let work = if qty > 0. && rate > 0. {
        BigRational::from_float(qty)? / BigRational::from_float(rate)?
    } else {
        BigRational::zero()
    };
    let value =
        (BigRational::from_float(lag)? + work) * BigRational::from_integer(BigInt::from(60));
    let n = value.numer().abs();
    let d = value.denom();
    let mut q = &n / d;
    let remainder = &n % d;
    let twice = &remainder * BigInt::from(2);
    if twice > *d || (twice == *d && (&q % BigInt::from(2)) != BigInt::zero()) {
        q += 1;
    }
    if value.is_negative() {
        q = -q;
    }
    q.to_i64()
}
/// Durations of activities driven by their assignments' units and rates.
/// Unsupported assignment shapes keep their written durations; the
/// scheduler receives minutes.
fn preprocess_units(
    doc: &Document,
    m: &mut Model,
    details: &mut BTreeMap<i64, ActivityDetails>,
    import: bool,
) {
    let Some(tasks) = doc.table("TASK") else {
        return;
    };
    let Some(assignments) = doc.table("TASKRSRC") else {
        return;
    };
    let pid = m.project.proj_id;
    let linked = doc
        .table("PROJECT")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, "rem_target_link_flag") == "Y")
        })
        .unwrap_or(false);
    let resource_types: HashMap<i64, &str> = doc.table("RSRC").map_or_else(HashMap::new, |t| {
        t.rows
            .iter()
            .map(|r| (num(t.value(r, "rsrc_id")), t.value(r, "rsrc_type")))
            .collect()
    });
    let task_cols = tasks.field_index();
    let t_proj = task_cols.col("proj_id");
    let t_id = task_cols.col("task_id");
    let t_dur = task_cols.col("duration_type");
    let t_pct = task_cols.col("complete_pct_type");
    let t_cstr = task_cols.col("cstr_type");
    let t_susp = task_cols.col("suspend_date");
    let mut task_rows: HashMap<i64, &Vec<String>> = HashMap::new();
    for r in &tasks.rows {
        if num(FieldIndex::get(r, t_proj)) == pid {
            task_rows.entry(num(FieldIndex::get(r, t_id))).or_insert(r);
        }
    }
    let asg_cols = assignments.field_index();
    let a_proj = asg_cols.col("proj_id");
    let a_task = asg_cols.col("task_id");
    let a_flag = asg_cols.col("rollup_dates_flag");
    let a_type = asg_cols.col("rsrc_type");
    let a_rsrc = asg_cols.col("rsrc_id");
    let a_relag = asg_cols.col("relag_drtn_hr_cnt");
    let a_qty = asg_cols.col("remain_qty");
    let a_rate = asg_cols.col("remain_qty_per_hr");
    let mut assignment_rows: HashMap<i64, Vec<&Vec<String>>> = HashMap::new();
    for r in &assignments.rows {
        if num(FieldIndex::get(r, a_proj)) == pid {
            assignment_rows
                .entry(num(FieldIndex::get(r, a_task)))
                .or_default()
                .push(r);
        }
    }
    for a in &mut m.activities {
        if a.task_type != TaskType::Task || a.is_completed() {
            continue;
        }
        let Some(&raw) = task_rows.get(&a.task_id) else {
            continue;
        };
        let dur_type = FieldIndex::get(raw, t_dur);
        let started = a.status == StatusType::InProgress;
        // Fixed Duration and Units: F9 sets a started activity's remaining
        // duration from its driving assignments' remaining units / units per
        // hour, to the whole minute (saved probe 1301b).
        let units_and_duration = import && dur_type == "DT_FixedDUR2" && started;
        if !units_and_duration
            && !matches!(dur_type, "DT_FixedDrtn" | "DT_FixedRate" | "DT_FixedQty")
        {
            continue;
        }
        if !started && !linked {
            continue;
        }
        let mut work = vec![];
        let mut supported = true;
        for r in assignment_rows.get(&a.task_id).into_iter().flatten() {
            let flag = FieldIndex::get(r, a_flag);
            let rsrc_type = FieldIndex::get(r, a_type);
            let kind = if rsrc_type.is_empty() {
                resource_types
                    .get(&num(FieldIndex::get(r, a_rsrc)))
                    .copied()
                    .unwrap_or("")
            } else {
                rsrc_type
            };
            if !matches!(flag, "Y" | "N") || kind != "RT_Labor" {
                supported = false;
                break;
            }
            if flag == "N" {
                continue;
            }
            let lag = if started {
                0.
            } else {
                FieldIndex::get(r, a_relag).parse::<f64>().unwrap_or(0.)
            };
            let qty = FieldIndex::get(r, a_qty).parse::<f64>().unwrap_or(0.);
            let rate = FieldIndex::get(r, a_rate).parse::<f64>().unwrap_or(0.);
            if lag != 0. && rate <= 0. {
                supported = false;
                break;
            }
            if let Some(minutes) = units_minutes(qty, rate, lag) {
                work.push(minutes);
            } else {
                supported = false;
                break;
            }
        }
        if !supported {
            continue;
        }
        let Some(rem) = work.into_iter().max() else {
            continue;
        };
        let pct_type = FieldIndex::get(raw, t_pct);
        let zero_proven = (pct_type == "CP_Drtn" || (started && pct_type == "CP_Units"))
            && FieldIndex::get(raw, t_cstr).is_empty()
            && a.cstr2 == CstrType::None
            && a.expect_end.is_none()
            && FieldIndex::get(raw, t_susp).is_empty()
            && a.resume.is_none();
        if rem == 0 && !zero_proven {
            continue;
        }
        a.rem = rem;
        if !started {
            if let Some(d) = details.get_mut(&a.task_id) {
                d.original_duration = rem;
            }
        }
    }
}
#[cfg(test)]
mod units_tests {
    use super::*;
    #[test]
    fn quotient_and_half_even_match_reference() {
        assert_eq!(units_minutes(21.65, 2., 0.), Some(649));
        assert_eq!(units_minutes(125.15, 2., 0.), Some(3755));
        assert_eq!(units_minutes(20.25, 2., 0.), Some(608));
        assert_eq!(units_minutes(20.75, 2., 0.), Some(622));
        assert_eq!(units_minutes(21.65, 2., 3.), Some(829));
        for a in 0..100 {
            for b in 1..25 {
                let qty = a as f64 * 0.25;
                let rate = b as f64 * 0.5;
                for lag in [0., 0.25, 1.5, -0.5] {
                    assert_eq!(
                        units_minutes(qty, rate, lag),
                        units_minutes_exact(qty, rate, lag),
                        "qty={qty} rate={rate} lag={lag}"
                    );
                }
            }
        }
    }
    fn with_assignment(status: &str, kind: &str, flag: &str) -> ProjectData {
        let mut doc = starter().document;
        let tasks = doc.table_mut("TASK");
        tasks.set(1, "duration_type", "DT_FixedQty".into());
        tasks.set(1, "complete_pct_type", "CP_Drtn".into());
        tasks.set(1, "status_code", status.into());
        doc.table_mut("TASKRSRC").add(&[
            ("task_id", "1010".into()),
            ("proj_id", "1".into()),
            ("rsrc_id", "9".into()),
            ("rsrc_type", kind.into()),
            ("rollup_dates_flag", flag.into()),
            ("remain_qty", "21.65".into()),
            ("remain_qty_per_hr", "2".into()),
            ("relag_drtn_hr_cnt", "3".into()),
        ]);
        ProjectData::from_document(doc, 1, "test".into()).unwrap()
    }
    /// Saved probe 1301b: under Fixed Duration and Units a started activity's
    /// remaining duration becomes its driving assignment's remaining units /
    /// units per hour (11.82 / 3.0164 h = 235 minutes); a not-started one
    /// keeps its own duration.
    #[test]
    fn fixed_duration_and_units_takes_started_remaining_duration_from_units() {
        let build = |status: &str| {
            let mut doc = starter().document;
            let tasks = doc.table_mut("TASK");
            tasks.set(1, "duration_type", "DT_FixedDUR2".into());
            tasks.set(1, "status_code", status.into());
            doc.table_mut("TASKRSRC").add(&[
                ("task_id", "1010".into()),
                ("proj_id", "1".into()),
                ("rsrc_id", "9".into()),
                ("rsrc_type", "RT_Labor".into()),
                ("rollup_dates_flag", "Y".into()),
                ("remain_qty", "11.82".into()),
                ("remain_qty_per_hr", "3.0164".into()),
            ]);
            ProjectData::from_document(doc, 1, "test".into()).unwrap()
        };
        let before = starter().model.activities[1].rem;
        assert_ne!(before, 235);
        assert_eq!(build("TK_Active").model.activities[1].rem, 235);
        assert_eq!(build("TK_NotStart").model.activities[1].rem, before);
    }
    #[test]
    fn unit_duration_and_started_lag() {
        let p = with_assignment("TK_NotStart", "RT_Labor", "Y");
        assert_eq!(p.model.activities[1].rem, 829);
        assert!(p.details[&1010].resource_driven_duration);
        assert!(!starter().details[&1010].resource_driven_duration);
        let reopened =
            ProjectData::from_document(p.updated_document(), 1, "roundtrip".into()).unwrap();
        assert!(reopened.details[&1010].resource_driven_duration);
        assert_eq!(reopened.model.activities[1].rem, 829);
        assert_eq!(p.details[&1010].original_duration, 829);
        let p = with_assignment("TK_Active", "RT_Labor", "Y");
        assert_eq!(p.model.activities[1].rem, 649);
        assert_eq!(p.details[&1010].original_duration, 1440);
    }
    #[test]
    fn status_edit_refreshes_units_before_scheduling_and_reopen() {
        let mut project = with_assignment("TK_NotStart", "RT_Labor", "Y");
        assert_eq!(project.model.activities[1].rem, 829);
        project.model.activities[1].status = StatusType::InProgress;
        project.model.activities[1].act_start = project.model.project.data_date;
        project.refresh_derived_durations();
        assert_eq!(project.model.activities[1].rem, 649);
        openxer_engine::scheduler::schedule_with_resources(&mut project.model, &project.resources)
            .unwrap();
        let reopened =
            ProjectData::from_document(project.updated_document(), 1, "scheduled".into()).unwrap();
        assert_eq!(reopened.model.activities[1].rem, 649);
        assert_eq!(
            reopened.model.activities[1].early_finish,
            project.model.activities[1].early_finish
        );
    }
    #[test]
    fn unsupported_and_non_driving_assignments_preserve_duration() {
        assert_eq!(
            with_assignment("TK_NotStart", "RT_Equip", "Y")
                .model
                .activities[1]
                .rem,
            1440
        );
        assert_eq!(
            with_assignment("TK_NotStart", "RT_Labor", "N")
                .model
                .activities[1]
                .rem,
            1440
        );
    }
}

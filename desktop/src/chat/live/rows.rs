//! What the window shows of a proposal and of a tool call: the change rows
//! of an edit's field diff (`openxer_automation::edit::diff`, the report of
//! every apply), and a few plain words per chat tool.
use crate::chat::backend::ChangeRow;
use serde_json::Value;

/// Fields the schedule itself works out: after a recalculation they change
/// on many activities, so they are folded into one row.
const COMPUTED: &[&str] = &[
    "early_start_date",
    "early_end_date",
    "late_start_date",
    "late_end_date",
    "rem_late_start_date",
    "rem_late_end_date",
    "restart_date",
    "reend_date",
    "total_float_hr_cnt",
    "free_float_hr_cnt",
    "driving_path_flag",
    "float_path",
    "float_path_order",
    "target_start_date",
    "target_end_date",
    "scd_end_date",
    "last_recalc_date",
    "next_data_date",
    "expect_end_date",
];

/// Plain names of the fields a chat edit usually changes.
const FIELDS: &[(&str, &str)] = &[
    ("task_name", "Activity Name"),
    ("task_code", "Activity ID"),
    ("target_drtn_hr_cnt", "Original Duration"),
    ("remain_drtn_hr_cnt", "Remaining Duration"),
    ("act_start_date", "Actual Start"),
    ("act_end_date", "Actual Finish"),
    ("phys_complete_pct", "Physical % Complete"),
    ("status_code", "Status"),
    ("cstr_type", "Primary Constraint"),
    ("cstr_date", "Primary Constraint Date"),
    ("clndr_id", "Calendar"),
    ("wbs_id", "WBS"),
    ("lag_hr_cnt", "Lag"),
    ("pred_type", "Relationship Type"),
    ("target_qty", "Budgeted Units"),
    ("remain_qty", "Remaining Units"),
    ("last_recalc_date", "Last Scheduled"),
    ("plan_end_date", "Must Finish By"),
    ("last_update_date", "Data Date"),
];

fn field_name(field: &str) -> String {
    FIELDS
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| field.to_string())
}

/// The thing a diff key names, in the window's words: an Activity ID, a
/// relationship "A1010 to A1020 (FS)", an assignment "A1010 FITTER".
fn subject(label: &str, key: &str) -> String {
    let parts: Vec<&str> = key.split('|').collect();
    match (label, parts.as_slice()) {
        ("relationships", [pred, succ, kind]) => {
            format!("{pred} to {succ} ({})", kind.trim_start_matches("PR_"))
        }
        ("assignments", [activity, resource]) => format!("{activity} {resource}"),
        ("activities", _) => key.to_string(),
        (_, _) => format!("{} {key}", label.replace('_', " ")),
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The change rows of an edit's field diff: one per changed field, one per
/// added or removed record, and the dates and floats a recalculation moved
/// as one closing row.
pub(crate) fn rows(diff: &Value) -> Vec<ChangeRow> {
    let mut out = vec![];
    let mut recalculated = std::collections::BTreeSet::new();
    let Some(groups) = diff.as_object() else {
        return out;
    };
    for (label, group) in groups {
        if !group.is_object() {
            continue;
        }
        for key in group["added"].as_array().into_iter().flatten() {
            out.push(ChangeRow {
                activity: subject(label, &text(key)),
                field: "Added".into(),
                old: String::new(),
                new: "new".into(),
            });
        }
        for key in group["removed"].as_array().into_iter().flatten() {
            out.push(ChangeRow {
                activity: subject(label, &text(key)),
                field: "Deleted".into(),
                old: "exists".into(),
                new: String::new(),
            });
        }
        for (key, fields) in group["changed"].as_object().into_iter().flatten() {
            for (field, change) in fields.as_object().into_iter().flatten() {
                if COMPUTED.contains(&field.as_str()) {
                    recalculated.insert(subject(label, key));
                    continue;
                }
                out.push(ChangeRow {
                    activity: subject(label, key),
                    field: field_name(field),
                    old: text(&change["before"]),
                    new: text(&change["after"]),
                });
            }
        }
    }
    if !recalculated.is_empty() {
        let n = recalculated.len();
        out.push(ChangeRow {
            activity: "Schedule".into(),
            field: "Dates and float".into(),
            old: String::new(),
            new: format!(
                "recalculated for {n} {}",
                if n == 1 { "item" } else { "items" }
            ),
        });
    }
    out
}

/// Short one-line schedule impact headline from a proposal's diff: project
/// finish date movement, critical/driving shifts and counts of edited records.
pub(crate) fn impact_summary(diff: &Value) -> Option<String> {
    let groups = diff.as_object()?;
    let mut parts: Vec<String> = vec![];
    if let Some(proj) = groups.get("project").and_then(|g| g["changed"].as_object()) {
        for fields in proj.values() {
            if let Some(scd) = fields.get("scd_end_date") {
                let before = text(&scd["before"]);
                let after = text(&scd["after"]);
                if !before.is_empty() && !after.is_empty() && before != after {
                    parts.push(format!("Finish: {before} \u{2192} {after}"));
                }
            }
            if let Some(plan) = fields.get("plan_end_date") {
                let before = text(&plan["before"]);
                let after = text(&plan["after"]);
                if before != after {
                    parts.push(format!("Must Finish: {before} \u{2192} {after}"));
                }
            }
        }
    }
    let mut driving_added = 0usize;
    let mut driving_removed = 0usize;
    let mut float_shifted = 0usize;
    let mut act_edited = 0usize;
    if let Some(acts) = groups.get("activities").and_then(Value::as_object) {
        let added = acts["added"].as_array().map_or(0, Vec::len);
        let removed = acts["removed"].as_array().map_or(0, Vec::len);
        if added > 0 {
            parts.push(format!(
                "+{added} activit{}",
                if added == 1 { "y" } else { "ies" }
            ));
        }
        if removed > 0 {
            parts.push(format!(
                "-{removed} activit{}",
                if removed == 1 { "y" } else { "ies" }
            ));
        }
        if let Some(changed) = acts["changed"].as_object() {
            for fields in changed.values() {
                let Some(map) = fields.as_object() else {
                    continue;
                };
                if map.keys().any(|k| !COMPUTED.contains(&k.as_str())) {
                    act_edited += 1;
                }
                if let Some(d) = map.get("driving_path_flag") {
                    let b = text(&d["before"]);
                    let a = text(&d["after"]);
                    if a == "Y" && b != "Y" {
                        driving_added += 1;
                    } else if b == "Y" && a != "Y" {
                        driving_removed += 1;
                    }
                }
                if map.contains_key("total_float_hr_cnt") || map.contains_key("early_end_date") {
                    float_shifted += 1;
                }
            }
        }
    }
    if act_edited > 0 {
        parts.push(format!(
            "{act_edited} activit{} edited",
            if act_edited == 1 { "y" } else { "ies" }
        ));
    }
    if let Some(rels) = groups.get("relationships").and_then(Value::as_object) {
        let added = rels["added"].as_array().map_or(0, Vec::len);
        let removed = rels["removed"].as_array().map_or(0, Vec::len);
        let changed = rels["changed"].as_object().map_or(0, serde_json::Map::len);
        let total = added + removed + changed;
        if total > 0 {
            parts.push(format!(
                "{total} link{} changed",
                if total == 1 { "" } else { "s" }
            ));
        }
    }
    if driving_added > 0 || driving_removed > 0 {
        parts.push(format!("Driving path: +{driving_added}/-{driving_removed}"));
    }
    if float_shifted > 0 {
        parts.push(format!(
            "{float_shifted} date/float shift{}",
            if float_shifted == 1 { "" } else { "s" }
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

/// A chat tool as the tool rows name it.
pub(crate) fn tool_label(tool: &str) -> String {
    match tool {
        "openxer_load" => "read schedule".into(),
        "openxer_records" => "read records".into(),
        "openxer_earned_value" => "earned value".into(),
        "openxer_usage" => "usage".into(),
        "openxer_documents" => "documents".into(),
        "openxer_read_document" => "read document".into(),
        "openxer_curves" => "resource curves".into(),
        openxer_automation::chat_tools::UI_TOOL => "app action".into(),
        openxer_automation::chat_tools::ASK_USER_TOOL => "ask user".into(),
        openxer_automation::chat_tools::APPLY_TOOL => "propose changes".into(),
        other => other
            .strip_prefix("openxer_")
            .unwrap_or(other)
            .replace('_', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_diff_reads_as_plain_change_rows() {
        let diff = json!({
            "activities": {"added": ["A1050"], "removed": [], "changed": {
                "A1010": {"task_name": {"before": "Old", "after": "New"},
                          "early_start_date": {"before": "x", "after": "y"}},
                "A1020": {"early_end_date": {"before": "x", "after": "y"}}}},
            "relationships": {"added": ["A1010|A1020|PR_FS"], "removed": [], "changed": {}},
            "changed_field_count": 4
        });
        let r = rows(&diff);
        let line = |r: &ChangeRow| format!("{}|{}|{}|{}", r.activity, r.field, r.old, r.new);
        assert_eq!(
            r.iter().map(line).collect::<Vec<_>>(),
            [
                "A1050|Added||new",
                "A1010|Activity Name|Old|New",
                "A1010 to A1020 (FS)|Added||new",
                "Schedule|Dates and float||recalculated for 2 items",
            ]
        );
        assert!(rows(&json!(null)).is_empty());
        assert_eq!(tool_label("openxer_load"), "read schedule");
        assert_eq!(tool_label("openxer_apply"), "propose changes");
        let summary_diff = json!({
            "project": {"added": [], "removed": [], "changed": {
                "1": {"scd_end_date": {"before": "2026-06-09 17:00", "after": "2026-06-04 17:00"}}
            }},
            "activities": {"added": [], "removed": [], "changed": {
                "A1010": {
                    "remain_drtn_hr_cnt": {"before": "56", "after": "24"},
                    "total_float_hr_cnt": {"before": "0", "after": "40"},
                    "driving_path_flag": {"before": "Y", "after": "N"}
                }
            }}
        });
        let summary = impact_summary(&summary_diff).expect("summary");
        assert!(summary.contains("Finish: 2026-06-09 17:00 \u{2192} 2026-06-04 17:00"));
        assert!(summary.contains("1 activity edited"));
        assert!(summary.contains("Driving path: +0/-1"));
        assert!(summary.contains("1 date/float shift"));
    }
}

//! `load`: project options and paged activity and relationship rows,
//! without scheduling. Fields, Activity IDs, a WBS element and critical
//! only keep an answer small when an agent needs only a few things, and a
//! sort field answers "which is latest" in one page.
use crate::{activity_json, documents, link_json, project_json, AutomationError, Result};
use openxer_xer::ProjectData;
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Largest page of activities or relationships.
pub const MAX_LIMIT: usize = 2000;

/// The activity fields `fields` can name. The fields in [`ON_REQUEST`] are
/// only given when named.
pub const FIELDS: &[&str] = &[
    "task_id",
    "task_code",
    "name",
    "task_type",
    "status",
    "calendar_id",
    "wbs",
    "critical",
    "early_start",
    "early_finish",
    "late_start",
    "late_finish",
    "remaining_start",
    "remaining_finish",
    "remaining_duration_minutes",
    "total_float_minutes",
    "free_float_minutes",
    "driving_path",
    "float_path",
    "float_path_order",
    "documents",
    "original_duration_minutes",
    "actual_start",
    "actual_finish",
    "constraint",
    "constraint_date",
];

/// Fields given only when `fields` names them: the WBS code path, critical
/// by the project's own definition, the original duration in working
/// minutes, the actual dates and the primary constraint with its date.
pub const ON_REQUEST: &[&str] = &[
    "wbs",
    "critical",
    "original_duration_minutes",
    "actual_start",
    "actual_finish",
    "constraint",
    "constraint_date",
];

/// The value of an [`ON_REQUEST`] field.
fn on_request(
    p: &ProjectData,
    a: &openxer_engine::model::Activity,
    field: &str,
    definition: openxer_xer::critical::CriticalDefinition,
) -> Value {
    let date = |v: Option<i64>| {
        v.map_or(Value::Null, |v| {
            json!(openxer_xer::date_string(p.model.origin_serial, Some(v)))
        })
    };
    match field {
        "wbs" => json!(p.details.get(&a.task_id).map_or(String::new(), |d| {
            openxer_xer::edit::documents::wbs_path(p, d.wbs)
        })),
        "critical" => json!(openxer_xer::critical::is_critical(a, definition)),
        "original_duration_minutes" => {
            json!(p.details.get(&a.task_id).map(|d| d.original_duration))
        }
        "actual_start" => date(a.act_start),
        "actual_finish" => date(a.act_finish),
        "constraint" => match a.cstr {
            openxer_engine::model::CstrType::None => Value::Null,
            c => json!(openxer_xer::constraint_label(c)),
        },
        "constraint_date" => date(a.cstr_date),
        _ => Value::Null,
    }
}

/// What to load. A limit of 0 returns no rows of that kind (counts only).
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub offset: usize,
    pub limit: usize,
    pub relationship_offset: usize,
    pub relationship_limit: usize,
    pub fields: Option<&'a [String]>,
    pub activities: Option<&'a [String]>,
    pub wbs: Option<&'a str>,
    pub critical: bool,
    /// Order the activities by this field (`-` first for descending);
    /// empty values go last either way. Project order when omitted.
    pub sort: Option<&'a str>,
}

/// Order two WBS code paths element by element: numbers by value where
/// both codes are numbers (`1.2` before `1.10`), else by their text.
pub fn compare_wbs_paths(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n).then_with(|| x.cmp(y)),
                    _ => x.cmp(y),
                };
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Order two field values: numbers by value, text by its characters
/// (dates are `YYYY-MM-DD HH:MM`, so they sort as dates); empty last.
fn compare(a: &Value, b: &Value, descending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let empty = |v: &Value| v.is_null() || v.as_str() == Some("");
    match (empty(a), empty(b)) {
        (true, true) => return Equal,
        (true, false) => return Greater,
        (false, true) => return Less,
        _ => {}
    }
    let order = match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Equal),
        _ => match (a.as_bool(), b.as_bool()) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => a.to_string().cmp(&b.to_string()),
        },
    };
    if descending {
        order.reverse()
    } else {
        order
    }
}

fn invalid(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

/// The `load` result for `p` (without `workspace_id`, which the caller
/// knows). With an activity filter (activities, wbs or critical) the
/// activities are the matching ones, and the relationships those with an
/// end among them; `matched_activities` and `matched_relationships` count
/// them, while the totals stay the project's.
pub fn load(p: &ProjectData, request: &Request) -> Result<Value> {
    if request.limit > MAX_LIMIT || request.relationship_limit > MAX_LIMIT {
        return Err(invalid(format!(
            "limit and relationship_limit must be 0..{MAX_LIMIT}; use offsets to page"
        )));
    }
    if let Some(fields) = request.fields {
        let unknown: Vec<&str> = fields
            .iter()
            .map(String::as_str)
            .filter(|f| !FIELDS.contains(f))
            .collect();
        if !unknown.is_empty() {
            return Err(invalid(format!(
                "Unknown field {}; fields are {}",
                unknown.join(", "),
                FIELDS.join(", ")
            )));
        }
    }
    let wbs_target = match request.wbs {
        Some(path) => Some(
            openxer_xer::edit::wbs::find_path(p, path)
                .ok_or_else(|| invalid(format!("No WBS element has the code path {path:?}")))?,
        ),
        None => None,
    };
    let wanted: Option<BTreeSet<&str>> = request
        .activities
        .map(|list| list.iter().map(String::as_str).collect());
    let not_found: Vec<&str> = wanted
        .iter()
        .flatten()
        .copied()
        .filter(|code| !p.model.activities.iter().any(|a| a.task_code == *code))
        .collect();
    let definition = openxer_xer::critical::critical_definition(p);
    let under = |mut wbs: i64, target: i64| {
        for _ in 0..=p.wbs.len() {
            if wbs == target {
                return true;
            }
            match p.wbs.iter().find(|w| w.id == wbs).and_then(|w| w.parent) {
                Some(parent) => wbs = parent,
                None => return false,
            }
        }
        false
    };
    let sort = match request.sort {
        Some(text) => {
            let (field, descending) = match text.strip_prefix('-') {
                Some(field) => (field, true),
                None => (text, false),
            };
            if !FIELDS.contains(&field) || field == "documents" {
                return Err(invalid(format!(
                    "Cannot sort by {field:?}; sort takes one of {} (- first for descending)",
                    FIELDS
                        .iter()
                        .filter(|f| **f != "documents")
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            Some((field, descending))
        }
        None => None,
    };
    let filtered = wanted.is_some() || wbs_target.is_some() || request.critical;
    let mut matched: Vec<usize> = (0..p.model.activities.len())
        .filter(|&i| {
            let a = &p.model.activities[i];
            wanted
                .as_ref()
                .is_none_or(|w| w.contains(a.task_code.as_str()))
                && wbs_target.is_none_or(|target| {
                    p.details
                        .get(&a.task_id)
                        .is_some_and(|d| under(d.wbs, target))
                })
                && (!request.critical || openxer_xer::critical::is_critical(a, definition))
        })
        .collect();
    if let Some((field, descending)) = sort {
        let key = |i: usize| -> Value {
            let a = &p.model.activities[i];
            if ON_REQUEST.contains(&field) {
                on_request(p, a, field, definition)
            } else {
                activity_json(p, a)[field].clone()
            }
        };
        let mut keyed: Vec<(Value, usize)> = matched.iter().map(|&i| (key(i), i)).collect();
        // Stable: equal values keep the project's order.
        if field == "wbs" {
            keyed.sort_by(|a, b| {
                let order =
                    compare_wbs_paths(a.0.as_str().unwrap_or(""), b.0.as_str().unwrap_or(""));
                if descending {
                    order.reverse()
                } else {
                    order
                }
            });
        } else {
            keyed.sort_by(|a, b| compare(&a.0, &b.0, descending));
        }
        matched = keyed.into_iter().map(|(_, i)| i).collect();
    }
    let links: Vec<usize> = if filtered {
        let set: BTreeSet<usize> = matched.iter().copied().collect();
        (0..p.model.links.len())
            .filter(|&i| {
                let l = &p.model.links[i];
                set.contains(&l.pred) || set.contains(&l.succ)
            })
            .collect()
    } else {
        (0..p.model.links.len()).collect()
    };
    if request.offset > matched.len() || request.relationship_offset > links.len() {
        return Err(invalid(
            "offset exceeds the corresponding activity or relationship count",
        ));
    }
    let end = request
        .offset
        .saturating_add(request.limit)
        .min(matched.len());
    let keep: Option<BTreeSet<&str>> = request
        .fields
        .map(|f| f.iter().map(String::as_str).chain(["task_code"]).collect());
    let wants = |field: &str| keep.as_ref().is_some_and(|k| k.contains(field));
    // Each activity lists the documents linked to it.
    let mut linked = if keep.is_none() || wants("documents") {
        documents::by_activity(p)
    } else {
        Default::default()
    };
    let activities: Vec<Value> = matched[request.offset..end]
        .iter()
        .map(|&i| {
            let a = &p.model.activities[i];
            let mut v = activity_json(p, a);
            v["documents"] = Value::Array(linked.remove(&a.task_id).unwrap_or_default());
            if let Some(keep) = &keep {
                for field in ON_REQUEST {
                    if keep.contains(field) {
                        v[*field] = on_request(p, a, field, definition);
                    }
                }
                if let Some(map) = v.as_object_mut() {
                    map.retain(|k, _| keep.contains(k.as_str()));
                }
            }
            v
        })
        .collect();
    let relationship_end = request
        .relationship_offset
        .saturating_add(request.relationship_limit)
        .min(links.len());
    let relationships: Vec<Value> = links[request.relationship_offset..relationship_end]
        .iter()
        .map(|&i| link_json(p, &p.model.links[i]))
        .collect();
    let mut value = json!({"project": project_json(p),
          "activities":activities,"relationships":relationships,"offset":request.offset,
          "next_offset":(request.limit > 0 && end < matched.len()).then_some(end),"total_activities":p.model.activities.len(),
          "relationship_offset":request.relationship_offset,
          "next_relationship_offset":(request.relationship_limit > 0 && relationship_end < links.len()).then_some(relationship_end),
          "total_relationships":p.model.links.len(),"warnings":p.warnings});
    if filtered {
        value["matched_activities"] = json!(matched.len());
        value["matched_relationships"] = json!(links.len());
    }
    if !not_found.is_empty() {
        value["not_found"] = json!(not_found);
    }
    Ok(value)
}

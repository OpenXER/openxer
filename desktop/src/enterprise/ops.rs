//! The enterprise dictionaries kept in the project document: OBS, roles with
//! prices and role codes, resource curves, resource shifts, currencies,
//! funding sources, financial period calendars and stored period
//! performance. Pure functions over the XER tables, so the dialogs, undo,
//! save and export all see the same rows.
use super::tree::{self, clean_text, Result, TreeSpec};
use crate::records::{self, float, num};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use openxer_xer::edit::curves;
use openxer_xer::Document;
use std::collections::BTreeMap;

// ------------------------------------------------------------------ specs

pub const OBS: TreeSpec = TreeSpec {
    table: "OBS",
    id: "obs_id",
    parent: "parent_obs_id",
    seq: "seq_num",
    name: "obs_name",
    code: None,
    owner: None,
    max_depth: 10,
    max_len: 100,
    noun: "OBS element",
    new_name: "New OBS Element",
    code_prefix: "",
    add_child: true,
};

pub const ROLES: TreeSpec = TreeSpec {
    table: "ROLES",
    id: "role_id",
    parent: "parent_role_id",
    seq: "seq_num",
    name: "role_name",
    code: Some("role_short_name"),
    owner: None,
    max_depth: 20,
    max_len: 40,
    noun: "role",
    new_name: "New Role",
    code_prefix: "R",
    add_child: true,
};

pub const ROLE_CODE_VALUES: TreeSpec = TreeSpec {
    table: "ROLECATVAL",
    id: "role_catg_id",
    parent: "parent_role_catg_id",
    seq: "seq_num",
    name: "role_catg_short_name",
    code: Some("role_catg_short_name"),
    owner: Some("role_catg_type_id"),
    max_depth: 20,
    max_len: 20,
    noun: "role code value",
    new_name: "New",
    code_prefix: "",
    add_child: false,
};

pub const FUNDING: TreeSpec = TreeSpec {
    table: "FUNDSRC",
    id: "fund_id",
    parent: "parent_fund_id",
    seq: "seq_num",
    name: "fund_name",
    code: None,
    owner: None,
    max_depth: 10,
    max_len: 100,
    noun: "funding source",
    new_name: "New Funding Source",
    code_prefix: "",
    add_child: true,
};

/// A flat dictionary: rows with an ID and a unique name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListSpec {
    pub table: &'static str,
    pub id: &'static str,
    pub name: &'static str,
    pub max_len: usize,
    pub noun: &'static str,
    pub new_name: &'static str,
}

pub const CURVES: ListSpec = ListSpec {
    table: "RSRCCURVDATA",
    id: "curv_id",
    name: "curv_name",
    max_len: 60,
    noun: "resource curve",
    new_name: "New Curve",
};
pub const SHIFTS: ListSpec = ListSpec {
    table: "SHIFT",
    id: "shift_id",
    name: "shift_name",
    max_len: 40,
    noun: "shift",
    new_name: "New Shift",
};
pub const CURRENCIES: ListSpec = ListSpec {
    table: "CURRTYPE",
    id: "curr_id",
    name: "curr_type",
    max_len: 40,
    noun: "currency",
    new_name: "New Currency",
};
pub const FIN_CALENDARS: ListSpec = ListSpec {
    table: "FINTMPL",
    id: "fintmpl_id",
    name: "fintmpl_name",
    max_len: 60,
    noun: "financial period calendar",
    new_name: "New Financial Period Calendar",
};
pub const ROLE_CODE_TYPES: ListSpec = ListSpec {
    table: "ROLECATTYPE",
    id: "role_catg_type_id",
    name: "role_catg_type",
    max_len: 40,
    noun: "role code",
    new_name: "New Role Code",
};

/// Most financial period calendars a database holds (Admin Preferences).
pub const MAX_FIN_CALENDARS: usize = 10;

// ------------------------------------------------------------ flat lists

/// IDs of a list's rows in file order.
pub fn list_ids(doc: &Document, spec: &ListSpec) -> Vec<i64> {
    doc.table(spec.table).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| num(t.value(r, spec.id)))
            .filter(|id| *id != 0)
            .collect()
    })
}

pub fn list_value(doc: &Document, spec: &ListSpec, id: i64, column: &str) -> String {
    doc.table(spec.table)
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, spec.id)) == id)
                .map(|r| t.value(r, column).to_string())
        })
        .unwrap_or_default()
}

fn list_row(doc: &Document, spec: &ListSpec, id: i64) -> Option<usize> {
    let t = doc.table(spec.table)?;
    t.rows.iter().position(|r| num(t.value(r, spec.id)) == id)
}

fn list_set(doc: &mut Document, spec: &ListSpec, id: i64, column: &str, text: String) {
    if let Some(i) = list_row(doc, spec, id) {
        doc.table_mut(spec.table).set(i, column, text);
    }
}

fn taken(doc: &Document, spec: &ListSpec, column: &str, id: Option<i64>, text: &str) -> bool {
    list_ids(doc, spec).into_iter().any(|other| {
        Some(other) != id
            && list_value(doc, spec, other, column)
                .trim()
                .eq_ignore_ascii_case(text.trim())
    })
}

fn unique_name(doc: &Document, spec: &ListSpec, column: &str, base: &str) -> String {
    if !taken(doc, spec, column, None, base) {
        return base.into();
    }
    (1..)
        .map(|k| format!("{base} {k}"))
        .find(|s| !taken(doc, spec, column, None, s))
        .unwrap()
}

/// Check a name for a list row: clean and unique.
pub fn check_list_name(
    doc: &Document,
    spec: &ListSpec,
    id: Option<i64>,
    text: &str,
) -> Result<String> {
    let name = clean_text(text, spec.max_len, &format!("{} name", spec.noun))?;
    if taken(doc, spec, spec.name, id, &name) {
        return Err(format!("A {} named \"{name}\" already exists.", spec.noun));
    }
    Ok(name)
}

/// Add a list row with a generated unique name and `extra` values.
pub fn list_add(doc: &mut Document, spec: &ListSpec, extra: &[(&str, String)]) -> i64 {
    let id = records::next_id(doc, spec.table, spec.id);
    let name = unique_name(doc, spec, spec.name, spec.new_name);
    let mut values = vec![(spec.id, id.to_string()), (spec.name, name)];
    for (k, v) in extra {
        values.retain(|(c, _)| c != k);
        values.push((k, v.clone()));
    }
    records::add_row(doc, spec.table, &values);
    id
}

/// Rename a list row; false when it already has that name.
pub fn list_rename(doc: &mut Document, spec: &ListSpec, id: i64, text: &str) -> Result<bool> {
    let name = check_list_name(doc, spec, Some(id), text)?;
    if list_value(doc, spec, id, spec.name) == name {
        return Ok(false);
    }
    list_set(doc, spec, id, spec.name, name);
    Ok(true)
}

pub fn list_delete(doc: &mut Document, spec: &ListSpec, id: i64) {
    tree::remove_uses(doc, spec.table, spec.id, &[id]);
}

/// Free text (descriptions): no control characters other than line breaks,
/// limited length; may be blank.
pub fn clean_note(text: &str, max_len: usize) -> Result<String> {
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r')
    {
        return Err("The text cannot contain control characters.".into());
    }
    if text.chars().count() > max_len {
        return Err(format!(
            "The text cannot be longer than {max_len} characters."
        ));
    }
    Ok(text.trim().to_string())
}

/// Set a free-text column of a tree row; false when unchanged.
pub fn set_tree_text(
    doc: &mut Document,
    spec: &TreeSpec,
    id: i64,
    column: &str,
    text: &str,
    max_len: usize,
) -> Result<bool> {
    let clean = clean_note(text, max_len)?;
    if tree::value(doc, spec, id, column) == clean {
        return Ok(false);
    }
    let Some(i) = doc
        .table(spec.table)
        .and_then(|t| t.rows.iter().position(|r| num(t.value(r, spec.id)) == id))
    else {
        return Ok(false);
    };
    doc.table_mut(spec.table).set(i, column, clean);
    Ok(true)
}

fn parse_number(text: &str, what: &str) -> Result<f64> {
    let t = text.trim().trim_start_matches('$').replace(',', "");
    let v: f64 = t
        .parse()
        .map_err(|_| format!("The {what} must be a number."))?;
    if !v.is_finite() {
        return Err(format!("The {what} must be a number."));
    }
    Ok(v)
}

// -------------------------------------------------------------------- OBS

/// Where rows that use a deleted OBS element go: its parent, else the first
/// other top-level element.
pub fn obs_heir(doc: &Document, id: i64) -> Option<i64> {
    let all = tree::nodes(doc, &OBS, None);
    let gone = tree::subtree(doc, &OBS, None, id);
    all.iter()
        .find(|n| n.id == id)
        .and_then(|n| n.parent)
        .or_else(|| {
            all.iter()
                .find(|n| n.depth == 0 && !gone.contains(&n.id))
                .map(|n| n.id)
        })
}

/// Tables and columns that name a responsible manager.
pub const OBS_USERS: [(&str, &str); 3] = [
    ("PROJWBS", "obs_id"),
    ("PROJISSU", "obs_id"),
    ("PROJTHRS", "obs_id"),
];

/// Rows that use the element or one under it.
pub fn obs_uses(doc: &Document, id: i64) -> usize {
    let ids = tree::subtree(doc, &OBS, None, id);
    OBS_USERS
        .iter()
        .map(|(t, c)| tree::uses(doc, t, c, &ids))
        .sum()
}

/// Delete an OBS element and the elements under it; their responsibilities
/// pass to [`obs_heir`].
pub fn delete_obs(doc: &mut Document, id: i64) -> Result<()> {
    if tree::nodes(doc, &OBS, None).len() <= tree::subtree(doc, &OBS, None, id).len() {
        return Err("The OBS needs at least one element.".into());
    }
    let heir = obs_heir(doc, id);
    let gone = tree::delete(doc, &OBS, None, id);
    for (t, c) in OBS_USERS {
        tree::retarget(doc, t, c, &gone, heir);
    }
    Ok(())
}

// ------------------------------------------------------------------ roles

pub const PRICE_COLUMNS: [&str; 5] = [
    "cost_per_qty",
    "cost_per_qty2",
    "cost_per_qty3",
    "cost_per_qty4",
    "cost_per_qty5",
];

fn role_rate_row(doc: &Document, role: i64) -> Option<usize> {
    let t = doc.table("ROLERATE")?;
    // The earliest rate is the role's price.
    t.rows
        .iter()
        .enumerate()
        .filter(|(_, r)| num(t.value(r, "role_id")) == role)
        .min_by_key(|(_, r)| t.value(r, "start_date").to_string())
        .map(|(i, _)| i)
}

/// The role's five prices per unit.
pub fn role_prices(doc: &Document, role: i64) -> [f64; 5] {
    let mut out = [0.; 5];
    if let (Some(t), Some(i)) = (doc.table("ROLERATE"), role_rate_row(doc, role)) {
        for (k, c) in PRICE_COLUMNS.iter().enumerate() {
            out[k] = float(t.value(&t.rows[i], c));
        }
    }
    out
}

/// Set price `k` (0 to 4) of a role; false when unchanged.
pub fn set_role_price(doc: &mut Document, role: i64, k: usize, text: &str) -> Result<bool> {
    let v = parse_number(text, "price per unit")?;
    if v < 0. {
        return Err("The price per unit cannot be negative.".into());
    }
    if (role_prices(doc, role)[k] - v).abs() < 1e-9 {
        return Ok(false);
    }
    let text = format!("{v:.4}");
    match role_rate_row(doc, role) {
        Some(i) => doc.table_mut("ROLERATE").set(i, PRICE_COLUMNS[k], text),
        None => {
            let id = records::next_id(doc, "ROLERATE", "role_rate_id");
            records::add_row(
                doc,
                "ROLERATE",
                &[
                    ("role_rate_id", id.to_string()),
                    ("role_id", role.to_string()),
                    (PRICE_COLUMNS[k], text),
                ],
            );
        }
    }
    Ok(true)
}

/// Rows that use the role or a role under it: assignments, resources' primary
/// roles and resource role lists.
pub fn role_uses(doc: &Document, id: i64) -> usize {
    let ids = tree::subtree(doc, &ROLES, None, id);
    tree::uses(doc, "TASKRSRC", "role_id", &ids)
        + tree::uses(doc, "RSRC", "role_id", &ids)
        + tree::uses(doc, "RSRCROLE", "role_id", &ids)
}

/// Delete a role and the roles under it with their prices, limits and codes;
/// assignments and resources that used them keep no role.
pub fn delete_role(doc: &mut Document, id: i64) {
    let gone = tree::delete(doc, &ROLES, None, id);
    for (t, c) in [
        ("ROLERATE", "role_id"),
        ("ROLELIMIT", "role_id"),
        ("ROLERCAT", "role_id"),
        ("RSRCROLE", "role_id"),
    ] {
        tree::remove_uses(doc, t, c, &gone);
    }
    tree::retarget(doc, "TASKRSRC", "role_id", &gone, None);
    tree::retarget(doc, "RSRC", "role_id", &gone, None);
}

// ------------------------------------------------------------- role codes

pub fn role_code_length(doc: &Document, type_id: i64) -> usize {
    let len = num(&list_value(
        doc,
        &ROLE_CODE_TYPES,
        type_id,
        "role_catg_short_len",
    ));
    if len <= 0 {
        20
    } else {
        len.min(20) as usize
    }
}

/// The value tree of one role code, limited to its Max Length.
pub fn role_value_spec(doc: &Document, type_id: i64) -> TreeSpec {
    TreeSpec {
        max_len: role_code_length(doc, type_id),
        ..ROLE_CODE_VALUES
    }
}

/// Role code types in sequence order.
pub fn role_code_types(doc: &Document) -> Vec<i64> {
    let mut ids = list_ids(doc, &ROLE_CODE_TYPES);
    ids.sort_by_key(|id| (num(&list_value(doc, &ROLE_CODE_TYPES, *id, "seq_num")), *id));
    ids
}

pub fn add_role_code_type(doc: &mut Document) -> i64 {
    let seq = role_code_types(doc)
        .iter()
        .map(|id| num(&list_value(doc, &ROLE_CODE_TYPES, *id, "seq_num")))
        .max()
        .unwrap_or(0)
        + 10;
    list_add(
        doc,
        &ROLE_CODE_TYPES,
        &[
            ("seq_num", seq.to_string()),
            ("role_catg_short_len", "10".into()),
        ],
    )
}

/// Set a role code's Max Length (1 to 20), refused when values are longer.
pub fn set_role_code_length(doc: &mut Document, type_id: i64, text: &str) -> Result<bool> {
    let len: usize = text
        .trim()
        .parse()
        .map_err(|_| "Max Length must be a whole number.".to_string())?;
    if !(1..=20).contains(&len) {
        return Err("Max Length must be between 1 and 20.".into());
    }
    let spec = role_value_spec(doc, type_id);
    let longest = tree::nodes(doc, &spec, Some(type_id))
        .iter()
        .map(|n| tree::value(doc, &spec, n.id, spec.name).chars().count())
        .max()
        .unwrap_or(0);
    if longest > len {
        return Err(format!(
            "A code value of this role code is {longest} characters long."
        ));
    }
    if role_code_length(doc, type_id) == len
        && !list_value(doc, &ROLE_CODE_TYPES, type_id, "role_catg_short_len").is_empty()
    {
        return Ok(false);
    }
    list_set(
        doc,
        &ROLE_CODE_TYPES,
        type_id,
        "role_catg_short_len",
        len.to_string(),
    );
    Ok(true)
}

/// Delete a role code with its values and their assignments to roles.
pub fn delete_role_code_type(doc: &mut Document, type_id: i64) {
    tree::remove_uses(doc, "ROLECATVAL", "role_catg_type_id", &[type_id]);
    tree::remove_uses(doc, "ROLERCAT", "role_catg_type_id", &[type_id]);
    list_delete(doc, &ROLE_CODE_TYPES, type_id);
}

/// Delete a code value and the values under it; roles lose those values.
pub fn delete_role_code_value(doc: &mut Document, type_id: i64, id: i64) {
    let spec = role_value_spec(doc, type_id);
    let gone = tree::delete(doc, &spec, Some(type_id), id);
    tree::remove_uses(doc, "ROLERCAT", "role_catg_id", &gone);
}

/// The value of role code `type_id` assigned to `role`.
pub fn role_code(doc: &Document, role: i64, type_id: i64) -> Option<i64> {
    let t = doc.table("ROLERCAT")?;
    t.rows
        .iter()
        .find(|r| {
            num(t.value(r, "role_id")) == role && num(t.value(r, "role_catg_type_id")) == type_id
        })
        .map(|r| num(t.value(r, "role_catg_id")))
}

/// Assign (or with `None` remove) a role's value of a role code.
pub fn set_role_code(doc: &mut Document, role: i64, type_id: i64, value: Option<i64>) -> bool {
    if role_code(doc, role, type_id) == value {
        return false;
    }
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("ROLERCAT"))
    {
        let (a, b) = (t.field("role_id"), t.field("role_catg_type_id"));
        t.rows.retain(|r| {
            let get = |k: Option<usize>| k.and_then(|k| r.get(k)).map_or(0, |v| num(v));
            !(get(a) == role && get(b) == type_id)
        });
    }
    if let Some(v) = value {
        records::add_row(
            doc,
            "ROLERCAT",
            &[
                ("role_id", role.to_string()),
                ("role_catg_type_id", type_id.to_string()),
                ("role_catg_id", v.to_string()),
            ],
        );
    }
    true
}

// ---------------------------------------------------------------- curves

pub const CURVE_POINTS: usize = curves::POINTS;

pub fn is_default_curve(doc: &Document, id: i64) -> bool {
    list_value(doc, &CURVES, id, "default_flag") == "Y"
}

/// The key the Resource Curves list uses for a preset the file does not
/// hold yet (file curves use their `curv_id`, always above zero).
fn preset_key(index: usize) -> i64 {
    -(index as i64) - 1
}

/// Every curve a user can pick, keyed for the list: the file's curves in
/// file order, then the presets the file does not hold (see
/// `openxer_xer::edit::curves`).
pub fn curve_rows(doc: &Document) -> Vec<(i64, curves::Curve)> {
    curves::list(doc)
        .into_iter()
        .map(|c| {
            let key = c.id.unwrap_or_else(|| {
                preset_key(
                    curves::presets()
                        .iter()
                        .position(|p| p.name == c.name)
                        .unwrap_or(0),
                )
            });
            (key, c)
        })
        .collect()
}

fn curve_of_key(doc: &Document, key: i64) -> Option<curves::Curve> {
    if key < 0 {
        let p = curves::presets().get((-key - 1) as usize)?;
        return curves::find(doc, p.name).filter(|c| c.id.is_none());
    }
    curves::by_id(doc, key)
}

/// Presets and default curves: viewed and copied, never changed.
pub fn is_locked_curve(doc: &Document, key: i64) -> bool {
    curve_of_key(doc, key).is_some_and(|c| c.locked())
}

/// Whether the curve is one of OpenXER's presets.
pub fn is_preset_curve(doc: &Document, key: i64) -> bool {
    curve_of_key(doc, key).is_some_and(|c| c.kind == curves::Kind::Preset)
}

/// A curve's name by list key.
pub fn curve_name(doc: &Document, key: i64) -> String {
    curve_of_key(doc, key).map_or_else(String::new, |c| c.name)
}

/// The 21 points of a curve: percent of the units spent in each 5% step
/// of the assignment duration (the first point is the start).
pub fn curve_points(doc: &Document, id: i64) -> [f64; CURVE_POINTS] {
    if id < 0 {
        return curve_of_key(doc, id).map_or([0.; CURVE_POINTS], |c| c.points);
    }
    std::array::from_fn(|k| float(&list_value(doc, &CURVES, id, &format!("pct_usage_{k}"))))
}

/// Rename a file curve. Presets and default curves keep their names, and
/// a file curve cannot take a preset's name (it would hide the preset).
pub fn rename_curve(doc: &mut Document, id: i64, text: &str) -> Result<bool> {
    if is_locked_curve(doc, id) {
        return Err("Preset and default resource curves cannot be renamed.".into());
    }
    let current = list_value(doc, &CURVES, id, "curv_name");
    if let Some(p) = curves::preset(text) {
        if !current.trim().eq_ignore_ascii_case(text.trim()) {
            return Err(format!(
                "\"{}\" is the name of a preset curve. Choose another name.",
                p.name
            ));
        }
    }
    list_rename(doc, &CURVES, id, text)
}

/// Curves offered to an assignment: (name, points, file ID or `None` for a
/// preset not in the file yet).
pub fn curve_choices(doc: &Document) -> Vec<(String, [f64; CURVE_POINTS], Option<i64>)> {
    curves::list(doc)
        .into_iter()
        .map(|c| (c.name, c.points, c.id))
        .collect()
}

/// A new curve: the selected curve's points, else an even spread.
pub fn add_curve(doc: &mut Document, from: Option<i64>) -> i64 {
    let points = match from {
        Some(id) => curve_points(doc, id),
        None => std::array::from_fn(|k| if k == 0 { 0. } else { 5. }),
    };
    let mut extra = vec![("default_flag", "N".to_string())];
    let names: Vec<String> = (0..CURVE_POINTS)
        .map(|k| format!("pct_usage_{k}"))
        .collect();
    for (k, name) in names.iter().enumerate() {
        extra.push((name.as_str(), number_text(points[k])));
    }
    list_add(doc, &CURVES, &extra)
}

fn number_text(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// Checks curve points typed in the editor and scales them to total 100%.
pub fn normalize_points(texts: &[String]) -> Result<[f64; CURVE_POINTS]> {
    if texts.len() != CURVE_POINTS {
        return Err("A resource curve has 21 points.".into());
    }
    let mut points = [0.; CURVE_POINTS];
    for (k, t) in texts.iter().enumerate() {
        let v = if t.trim().is_empty() {
            0.
        } else {
            parse_number(t.trim_end_matches('%'), "curve value")?
        };
        if !(0. ..=100.).contains(&v) {
            return Err("Curve values must be between 0 and 100.".into());
        }
        points[k] = v;
    }
    let total: f64 = points.iter().sum();
    if total <= 0. {
        return Err("A resource curve needs at least one value above zero.".into());
    }
    for p in &mut points {
        *p = (*p * 100. / total * 10000.).round() / 10000.;
    }
    Ok(points)
}

/// Store a curve's name and points; default curves cannot change.
pub fn set_curve(
    doc: &mut Document,
    id: i64,
    name: &str,
    points: &[f64; CURVE_POINTS],
) -> Result<bool> {
    if is_default_curve(doc, id) {
        return Err("Default resource curves cannot be changed.".into());
    }
    if is_locked_curve(doc, id) {
        return Err(
            "Preset curves cannot be changed. Click Add to make a copy you can edit.".into(),
        );
    }
    let mut changed = rename_curve(doc, id, name)?;
    for (k, p) in points.iter().enumerate() {
        let column = format!("pct_usage_{k}");
        let text = number_text(*p);
        if float(&list_value(doc, &CURVES, id, &column)) != *p
            || list_value(doc, &CURVES, id, &column).is_empty()
        {
            list_set(doc, &CURVES, id, &column, text);
            changed = true;
        }
    }
    Ok(changed)
}

/// Assignments that use the curve.
pub fn curve_uses(doc: &Document, id: i64) -> usize {
    tree::uses(doc, "TASKRSRC", "curv_id", &[id])
}

/// Delete a curve; its assignments fall back to no curve.
pub fn delete_curve(doc: &mut Document, id: i64) -> Result<()> {
    if is_default_curve(doc, id) {
        return Err("Default resource curves cannot be deleted.".into());
    }
    if is_locked_curve(doc, id) {
        return Err("Preset curves cannot be deleted.".into());
    }
    tree::retarget(doc, "TASKRSRC", "curv_id", &[id], None);
    list_delete(doc, &CURVES, id);
    Ok(())
}

/// The file's own curves: (ID, name). The picker uses [`curve_choices`],
/// which adds the presets.
#[cfg(test)]
pub fn curve_names(doc: &Document) -> Vec<(i64, String)> {
    list_ids(doc, &CURVES)
        .into_iter()
        .map(|id| (id, list_value(doc, &CURVES, id, "curv_name")))
        .collect()
}

// ---------------------------------------------------------------- shifts

/// A shift's periods: (period ID, start hour), by start hour.
pub fn shift_periods(doc: &Document, shift: i64) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = doc.table("SHIFTPER").map_or(vec![], |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "shift_id")) == shift)
            .map(|r| {
                (
                    num(t.value(r, "shift_period_id")),
                    num(t.value(r, "shift_start_hr_num")),
                )
            })
            .collect()
    });
    out.sort_by_key(|(id, start)| (*start, *id));
    out
}

/// Hours of each period: to the next period's start, the last one round to
/// the first.
pub fn period_hours(periods: &[(i64, i64)]) -> Vec<i64> {
    let n = periods.len();
    (0..n)
        .map(|k| {
            let start = periods[k].1;
            let next = if k + 1 < n {
                periods[k + 1].1
            } else {
                periods[0].1 + 24
            };
            next - start
        })
        .collect()
}

pub fn add_shift(doc: &mut Document) -> i64 {
    let id = list_add(doc, &SHIFTS, &[]);
    let period = records::next_id(doc, "SHIFTPER", "shift_period_id");
    records::add_row(
        doc,
        "SHIFTPER",
        &[
            ("shift_period_id", period.to_string()),
            ("shift_id", id.to_string()),
            ("shift_start_hr_num", "0".into()),
        ],
    );
    id
}

fn parse_hour(text: &str) -> Result<i64> {
    let h: i64 = text
        .trim()
        .trim_end_matches(":00")
        .parse()
        .map_err(|_| "A shift start is a whole hour from 0 to 23.".to_string())?;
    if !(0..=23).contains(&h) {
        return Err("A shift start is a whole hour from 0 to 23.".into());
    }
    Ok(h)
}

/// Add a period starting at the first free hour after the last start.
pub fn add_shift_period(doc: &mut Document, shift: i64) -> Result<i64> {
    let periods = shift_periods(doc, shift);
    if periods.len() >= 24 {
        return Err("A shift has at most 24 periods.".into());
    }
    let start = (0..24)
        .map(|k| (periods.last().map_or(0, |p| p.1) + 1 + k) % 24)
        .find(|h| !periods.iter().any(|p| p.1 == *h))
        .unwrap();
    let id = records::next_id(doc, "SHIFTPER", "shift_period_id");
    records::add_row(
        doc,
        "SHIFTPER",
        &[
            ("shift_period_id", id.to_string()),
            ("shift_id", shift.to_string()),
            ("shift_start_hr_num", start.to_string()),
        ],
    );
    Ok(id)
}

/// Move a period's start; starts stay unique within the shift.
pub fn set_period_start(doc: &mut Document, shift: i64, period: i64, text: &str) -> Result<bool> {
    let h = parse_hour(text)?;
    let periods = shift_periods(doc, shift);
    if periods.iter().any(|(id, s)| *id != period && *s == h) {
        return Err(format!("Another period of this shift starts at {h}."));
    }
    if periods.iter().any(|(id, s)| *id == period && *s == h) {
        return Ok(false);
    }
    let Some(i) = doc.table("SHIFTPER").and_then(|t| {
        t.rows
            .iter()
            .position(|r| num(t.value(r, "shift_period_id")) == period)
    }) else {
        return Ok(false);
    };
    doc.table_mut("SHIFTPER")
        .set(i, "shift_start_hr_num", h.to_string());
    Ok(true)
}

/// Delete a period (a shift keeps at least one); resource limits for it go.
pub fn delete_shift_period(doc: &mut Document, shift: i64, period: i64) -> Result<()> {
    if shift_periods(doc, shift).len() <= 1 {
        return Err("A shift needs at least one period.".into());
    }
    tree::remove_uses(doc, "SHIFTPER", "shift_period_id", &[period]);
    tree::retarget(doc, "RSRCRATE", "shift_period_id", &[period], None);
    Ok(())
}

/// Resources on the shift.
pub fn shift_uses(doc: &Document, shift: i64) -> usize {
    tree::uses(doc, "RSRC", "shift_id", &[shift])
}

/// Delete a shift with its periods; its resources keep no shift.
pub fn delete_shift(doc: &mut Document, shift: i64) {
    let periods: Vec<i64> = shift_periods(doc, shift).iter().map(|p| p.0).collect();
    tree::remove_uses(doc, "SHIFTPER", "shift_id", &[shift]);
    tree::retarget(doc, "RSRCRATE", "shift_period_id", &periods, None);
    tree::retarget(doc, "RSRC", "shift_id", &[shift], None);
    list_delete(doc, &SHIFTS, shift);
}

// ------------------------------------------------------------ currencies

/// Positive and negative currency patterns: `#` is the symbol, `1.1` the
/// number.
pub const POSITIVE_FORMATS: [&str; 4] = ["#1.1", "1.1#", "# 1.1", "1.1 #"];
pub const NEGATIVE_FORMATS: [&str; 8] = [
    "(#1.1)", "-#1.1", "#-1.1", "#1.1-", "(1.1#)", "-1.1#", "1.1-#", "1.1#-",
];

/// The base currency: the one the export header names, else the first with
/// an exchange rate of 1.
pub fn base_currency(doc: &Document) -> Option<i64> {
    let ids = list_ids(doc, &CURRENCIES);
    let header = doc
        .header
        .split('\t')
        .nth(8)
        .unwrap_or("")
        .trim()
        .to_string();
    ids.iter()
        .copied()
        .find(|id| {
            !header.is_empty()
                && list_value(doc, &CURRENCIES, *id, "curr_short_name").trim() == header
        })
        .or_else(|| {
            ids.iter()
                .copied()
                .find(|id| float(&list_value(doc, &CURRENCIES, *id, "base_exch_rate")) == 1.)
        })
        .or_else(|| ids.iter().copied().min())
}

pub fn add_currency(doc: &mut Document) -> i64 {
    let code = {
        let taken = |s: &str| {
            list_ids(doc, &CURRENCIES).iter().any(|id| {
                list_value(doc, &CURRENCIES, *id, "curr_short_name").eq_ignore_ascii_case(s)
            })
        };
        (1..).map(|k| format!("C{k}")).find(|s| !taken(s)).unwrap()
    };
    list_add(
        doc,
        &CURRENCIES,
        &[
            ("decimal_digit_cnt", "2".into()),
            ("curr_symbol", "$".into()),
            ("decimal_symbol", ".".into()),
            ("digit_group_symbol", ",".into()),
            ("pos_curr_fmt_type", "#1.1".into()),
            ("neg_curr_fmt_type", "(#1.1)".into()),
            ("curr_short_name", code),
            ("group_digit_cnt", "3".into()),
            ("base_exch_rate", "1".into()),
        ],
    )
}

/// Set one field of a currency, checked; false when unchanged.
pub fn set_currency(doc: &mut Document, id: i64, column: &str, text: &str) -> Result<bool> {
    let base = base_currency(doc) == Some(id);
    let value = match column {
        "curr_type" => check_list_name(doc, &CURRENCIES, Some(id), text)?,
        "curr_short_name" => {
            let code = clean_text(text, 10, "currency ID")?;
            let clash = list_ids(doc, &CURRENCIES).iter().any(|other| {
                *other != id
                    && list_value(doc, &CURRENCIES, *other, "curr_short_name")
                        .eq_ignore_ascii_case(&code)
            });
            if clash {
                return Err(format!("The currency ID \"{code}\" is already in use."));
            }
            code
        }
        "curr_symbol" => clean_text(text, 5, "currency symbol")?,
        "decimal_symbol" | "digit_group_symbol" => {
            let s = text.trim();
            if s.chars().count() != 1 || s.chars().any(|c| c.is_control() || c.is_ascii_digit()) {
                return Err("A symbol is one character that is not a digit.".into());
            }
            s.to_string()
        }
        "decimal_digit_cnt" => {
            let n: u8 = text
                .trim()
                .parse()
                .map_err(|_| "Decimal places must be a whole number.".to_string())?;
            if n > crate::format::CurrencyFormat::MAX_DECIMALS {
                return Err(format!(
                    "Decimal places must be from 0 to {}.",
                    crate::format::CurrencyFormat::MAX_DECIMALS
                ));
            }
            n.to_string()
        }
        "base_exch_rate" => {
            let v = parse_number(text, "exchange rate")?;
            if v <= 0. {
                return Err("The exchange rate must be greater than zero.".into());
            }
            if base && v != 1. {
                return Err("The base currency's exchange rate is always 1.".into());
            }
            number_text(v)
        }
        "pos_curr_fmt_type" if POSITIVE_FORMATS.contains(&text) => text.to_string(),
        "neg_curr_fmt_type" if NEGATIVE_FORMATS.contains(&text) => text.to_string(),
        _ => return Err(format!("{column} cannot be set here.")),
    };
    if list_value(doc, &CURRENCIES, id, column) == value {
        return Ok(false);
    }
    list_set(doc, &CURRENCIES, id, column, value);
    Ok(true)
}

/// Resources priced in the currency.
pub fn currency_uses(doc: &Document, id: i64) -> usize {
    tree::uses(doc, "RSRC", "curr_id", &[id])
}

/// Delete a currency; resources priced in it move to the base currency.
pub fn delete_currency(doc: &mut Document, id: i64) -> Result<()> {
    let base = base_currency(doc);
    if base == Some(id) {
        return Err("The base currency cannot be deleted.".into());
    }
    tree::retarget(doc, "RSRC", "curr_id", &[id], base);
    list_delete(doc, &CURRENCIES, id);
    Ok(())
}

/// The sample text the Appearance page shows: a positive and a negative
/// amount.
pub fn currency_example(doc: &Document, id: i64) -> String {
    let p = crate::format::CurrencyFormat {
        name: String::new(),
        symbol: list_value(doc, &CURRENCIES, id, "curr_symbol"),
        decimals: num(&list_value(doc, &CURRENCIES, id, "decimal_digit_cnt")).clamp(0, 4) as u8,
        decimal_symbol: list_value(doc, &CURRENCIES, id, "decimal_symbol")
            .chars()
            .next()
            .unwrap_or('.'),
        group_symbol: list_value(doc, &CURRENCIES, id, "digit_group_symbol")
            .chars()
            .next()
            .unwrap_or(','),
        group_digits: 3,
        positive: list_value(doc, &CURRENCIES, id, "pos_curr_fmt_type"),
        negative: list_value(doc, &CURRENCIES, id, "neg_curr_fmt_type"),
        show_symbol: true,
        show_decimals: true,
    };
    format!("{}   {}", p.format(123456.78), p.format(-123456.78))
}

// --------------------------------------------------------------- funding

/// Projects funded by the source or one under it.
pub fn funding_uses(doc: &Document, id: i64) -> usize {
    let ids = tree::subtree(doc, &FUNDING, None, id);
    tree::uses(doc, "PROJFUND", "fund_id", &ids)
}

/// Delete a funding source and those under it with the project funding rows
/// that name them.
pub fn delete_funding(doc: &mut Document, id: i64) {
    let gone = tree::delete(doc, &FUNDING, None, id);
    tree::remove_uses(doc, "PROJFUND", "fund_id", &gone);
}

// ------------------------------------------------------ financial periods

/// A financial period: ID, name and first and last day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Period {
    pub id: i64,
    pub name: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1899, 12, 30).unwrap()
}

/// The calendar day of an XER date.
pub fn xer_day(text: &str) -> Option<NaiveDate> {
    openxer_xer::parse_absolute(text).map(|m| epoch() + Duration::days(m.div_euclid(1440)))
}

/// The calendar day of absolute minutes (from 1899-12-30).
pub fn day_of_minutes(minutes: i64) -> NaiveDate {
    epoch() + Duration::days(minutes.div_euclid(1440))
}

fn start_text(d: NaiveDate) -> String {
    format!("{} 00:00", d.format("%Y-%m-%d"))
}

fn end_text(d: NaiveDate) -> String {
    format!("{} 23:59", d.format("%Y-%m-%d"))
}

/// Periods of a financial period calendar by start date.
pub fn periods(doc: &Document, calendar: i64) -> Vec<Period> {
    let mut out: Vec<Period> = doc.table("FINDATES").map_or(vec![], |t| {
        t.rows
            .iter()
            .filter(|r| {
                // Files without calendars of periods keep one set of periods.
                t.field("fintmpl_id").is_none() || num(t.value(r, "fintmpl_id")) == calendar
            })
            .filter_map(|r| {
                Some(Period {
                    id: num(t.value(r, "fin_dates_id")),
                    name: t.value(r, "fin_dates_name").to_string(),
                    start: xer_day(t.value(r, "start_date"))?,
                    end: xer_day(t.value(r, "end_date"))?,
                })
            })
            .collect()
    });
    out.sort_by_key(|p| (p.start, p.id));
    out
}

/// The periods that overlap the days `from` to `to` (User Preferences >
/// Financial Period Data); with no range every period is kept.
pub fn in_range(
    periods: Vec<Period>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Vec<Period> {
    periods
        .into_iter()
        .filter(|p| from.is_none_or(|f| p.end >= f) && to.is_none_or(|t| p.start <= t))
        .collect()
}

/// The default financial period calendar.
pub fn default_fin_calendar(doc: &Document) -> Option<i64> {
    let ids = list_ids(doc, &FIN_CALENDARS);
    ids.iter()
        .copied()
        .find(|id| list_value(doc, &FIN_CALENDARS, *id, "default_flag") == "Y")
        .or_else(|| ids.first().copied())
}

/// Add a calendar; the first one becomes the default.
pub fn add_fin_calendar(doc: &mut Document) -> Result<i64> {
    let ids = list_ids(doc, &FIN_CALENDARS);
    if ids.len() >= MAX_FIN_CALENDARS {
        return Err(format!(
            "A database holds at most {MAX_FIN_CALENDARS} financial period calendars."
        ));
    }
    let flag = if ids.is_empty() { "Y" } else { "N" };
    Ok(list_add(
        doc,
        &FIN_CALENDARS,
        &[("default_flag", flag.into())],
    ))
}

/// Make a calendar the default one.
pub fn set_default_fin_calendar(doc: &mut Document, id: i64) -> bool {
    if list_value(doc, &FIN_CALENDARS, id, "default_flag") == "Y" {
        return false;
    }
    for other in list_ids(doc, &FIN_CALENDARS) {
        let flag = if other == id { "Y" } else { "N" };
        list_set(doc, &FIN_CALENDARS, other, "default_flag", flag.into());
    }
    true
}

/// Rows that depend on the calendar: projects using it and stored
/// performance in its periods.
pub fn fin_calendar_uses(doc: &Document, id: i64) -> usize {
    let ids: Vec<i64> = periods(doc, id).iter().map(|p| p.id).collect();
    tree::uses(doc, "PROJECT", "fintmpl_id", &[id])
        + tree::uses(doc, "TASKFIN", "fin_dates_id", &ids)
}

/// Why a calendar whose periods hold stored values is not deleted.
pub const FIN_CALENDAR_KEPT: &str = "Stored period performance of activities or assignments \
    refers to periods of this calendar, so the calendar is kept. Only a calendar whose periods \
    hold no stored values can be deleted.";

/// Rows of stored period performance in the calendar's periods.
pub fn fin_calendar_stored(doc: &Document, id: i64) -> usize {
    let ids: Vec<i64> = periods(doc, id).iter().map(|p| p.id).collect();
    openxer_xer::edit::financial_periods::stored_rows(doc, &ids)
}

/// Delete a calendar with its periods; refused, with nothing changed, when
/// any of its periods holds stored period performance.
pub fn delete_fin_calendar(doc: &mut Document, id: i64) -> Result<()> {
    if fin_calendar_stored(doc, id) > 0 {
        return Err(FIN_CALENDAR_KEPT.into());
    }
    let ids: Vec<i64> = periods(doc, id).iter().map(|p| p.id).collect();
    delete_periods(doc, &ids)?;
    tree::retarget(doc, "PROJECT", "fintmpl_id", &[id], None);
    let was_default = list_value(doc, &FIN_CALENDARS, id, "default_flag") == "Y";
    list_delete(doc, &FIN_CALENDARS, id);
    if was_default {
        if let Some(first) = list_ids(doc, &FIN_CALENDARS).first().copied() {
            list_set(doc, &FIN_CALENDARS, first, "default_flag", "Y".into());
        }
    }
    Ok(())
}

/// Delete periods, all or none: refused when any holds stored period
/// performance (the shared rule of the command line and MCP too).
pub fn delete_periods(doc: &mut Document, ids: &[i64]) -> Result<()> {
    openxer_xer::edit::financial_periods::delete(doc, ids)
}

/// How Batch Create cuts a date range into periods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interval {
    /// Weeks ending on this weekday.
    Weekly(Weekday),
    /// Months ending on this day (31 means the last day of the month).
    Monthly(u32),
}

fn month_end(year: i32, month: u32, day: u32) -> NaiveDate {
    let last = (28..=31)
        .rev()
        .find_map(|d| NaiveDate::from_ymd_opt(year, month, d))
        .unwrap();
    NaiveDate::from_ymd_opt(year, month, day.min(last.day())).unwrap_or(last)
}

/// The end of the period that contains `day`.
fn period_end(day: NaiveDate, interval: Interval) -> NaiveDate {
    match interval {
        Interval::Weekly(end) => {
            let ahead = (7 + end.num_days_from_monday() as i64
                - day.weekday().num_days_from_monday() as i64)
                % 7;
            day + Duration::days(ahead)
        }
        Interval::Monthly(end_day) => {
            let this = month_end(day.year(), day.month(), end_day);
            if day <= this {
                this
            } else {
                let (y, m) = if day.month() == 12 {
                    (day.year() + 1, 1)
                } else {
                    (day.year(), day.month() + 1)
                };
                month_end(y, m, end_day)
            }
        }
    }
}

/// The periods Batch Create makes from `from` to `to`: contiguous, the first
/// starting on `from` and the last ending on or after `to`.
pub fn batch_periods(
    from: NaiveDate,
    to: NaiveDate,
    interval: Interval,
) -> Result<Vec<(NaiveDate, NaiveDate)>> {
    if to < from {
        return Err("The end date is before the start date.".into());
    }
    let mut out = vec![];
    let mut start = from;
    while start <= to {
        let end = period_end(start, interval);
        out.push((start, end));
        if out.len() > 1200 {
            return Err("That range makes more than 1,200 periods.".into());
        }
        start = end + Duration::days(1);
    }
    Ok(out)
}

/// The name of a period: its last day (weeks) or month (months).
pub fn period_name(end: NaiveDate, interval: Interval) -> String {
    match interval {
        Interval::Weekly(_) => end.format("%d-%b-%y").to_string(),
        Interval::Monthly(_) => end.format("%b-%y").to_string(),
    }
}

/// Batch Create: add the periods from `from` to `to`, refused when one would
/// overlap an existing period of the calendar. Returns the new IDs.
pub fn batch_create(
    doc: &mut Document,
    calendar: i64,
    from: NaiveDate,
    to: NaiveDate,
    interval: Interval,
) -> Result<Vec<i64>> {
    let new = batch_periods(from, to, interval)?;
    let (first, last) = (new[0].0, new[new.len() - 1].1);
    if let Some(p) = periods(doc, calendar)
        .iter()
        .find(|p| p.start <= last && p.end >= first)
    {
        return Err(format!(
            "The new periods overlap the period \"{}\". Delete the periods in that range first.",
            p.name
        ));
    }
    let mut ids = vec![];
    for (start, end) in new {
        let id = records::next_id(doc, "FINDATES", "fin_dates_id");
        records::add_row(
            doc,
            "FINDATES",
            &[
                ("fin_dates_id", id.to_string()),
                ("fintmpl_id", calendar.to_string()),
                ("fin_dates_name", period_name(end, interval)),
                ("start_date", start_text(start)),
                ("end_date", end_text(end)),
            ],
        );
        ids.push(id);
    }
    Ok(ids)
}

/// Rename a period: clean and unique within its calendar.
pub fn rename_period(doc: &mut Document, calendar: i64, id: i64, text: &str) -> Result<bool> {
    let name = clean_text(text, 40, "period name")?;
    let all = periods(doc, calendar);
    if all
        .iter()
        .any(|p| p.id != id && p.name.eq_ignore_ascii_case(&name))
    {
        return Err(format!("Another period is already named \"{name}\"."));
    }
    let Some(i) = doc.table("FINDATES").and_then(|t| {
        t.rows
            .iter()
            .position(|r| num(t.value(r, "fin_dates_id")) == id)
    }) else {
        return Ok(false);
    };
    if all.iter().any(|p| p.id == id && p.name == name) {
        return Ok(false);
    }
    doc.table_mut("FINDATES").set(i, "fin_dates_name", name);
    Ok(true)
}

/// The project's financial period calendar: its own, else the default.
pub fn project_fin_calendar(doc: &Document, pid: i64) -> Option<i64> {
    let own = doc.table("PROJECT").and_then(|t| {
        t.rows
            .iter()
            .find(|r| num(t.value(r, "proj_id")) == pid)
            .map(|r| num(t.value(r, "fintmpl_id")))
    });
    own.filter(|id| list_ids(doc, &FIN_CALENDARS).contains(id))
        .or_else(|| default_fin_calendar(doc))
}

/// Performance stored for one activity in one period.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stored {
    pub labor_units: f64,
    pub nonlabor_units: f64,
    pub labor_cost: f64,
    pub nonlabor_cost: f64,
    pub material_cost: f64,
    pub expense_cost: f64,
}

fn stored_before(doc: &Document, pid: i64, keep: &[i64]) -> BTreeMap<i64, Stored> {
    let mut out: BTreeMap<i64, Stored> = BTreeMap::new();
    if let Some(t) = doc.table("TASKFIN") {
        for r in t
            .rows
            .iter()
            .filter(|r| num(t.value(r, "proj_id")) == pid)
            .filter(|r| keep.contains(&num(t.value(r, "fin_dates_id"))))
        {
            let s = out.entry(num(t.value(r, "task_id"))).or_default();
            s.labor_units += float(t.value(r, "act_work_qty"));
            s.nonlabor_units += float(t.value(r, "act_equip_qty"));
            s.labor_cost += float(t.value(r, "act_work_cost"));
            s.nonlabor_cost += float(t.value(r, "act_equip_cost"));
            s.material_cost += float(t.value(r, "act_mat_cost"));
            s.expense_cost += float(t.value(r, "act_expense_cost"));
        }
    }
    out
}

fn assignment_before(doc: &Document, pid: i64, keep: &[i64]) -> BTreeMap<i64, (f64, f64)> {
    let mut out: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
    if let Some(t) = doc.table("TRSRCFIN") {
        for r in t
            .rows
            .iter()
            .filter(|r| num(t.value(r, "proj_id")) == pid)
            .filter(|r| keep.contains(&num(t.value(r, "fin_dates_id"))))
        {
            let e = out.entry(num(t.value(r, "taskrsrc_id"))).or_default();
            e.0 += float(t.value(r, "act_qty")) + float(t.value(r, "act_ot_qty"));
            e.1 += float(t.value(r, "act_cost")) + float(t.value(r, "act_ot_cost"));
        }
    }
    out
}

/// Actual to date per activity from the (exported) document: units from the
/// activity, costs from its assignments by resource type and its expenses.
fn actual_to_date(doc: &Document, pid: i64) -> BTreeMap<i64, Stored> {
    let mut out: BTreeMap<i64, Stored> = BTreeMap::new();
    if let Some(t) = doc.table("TASK") {
        for r in t.rows.iter().filter(|r| num(t.value(r, "proj_id")) == pid) {
            let s = out.entry(num(t.value(r, "task_id"))).or_default();
            s.labor_units = float(t.value(r, "act_work_qty"));
            s.nonlabor_units = float(t.value(r, "act_equip_qty"));
        }
    }
    if let Some(t) = doc.table("TASKRSRC") {
        for r in t.rows.iter().filter(|r| num(t.value(r, "proj_id")) == pid) {
            let cost = float(t.value(r, "act_reg_cost")) + float(t.value(r, "act_ot_cost"));
            let s = out.entry(num(t.value(r, "task_id"))).or_default();
            match t.value(r, "rsrc_type") {
                "RT_Equip" => s.nonlabor_cost += cost,
                "RT_Mat" => s.material_cost += cost,
                _ => s.labor_cost += cost,
            }
        }
    }
    if let Some(t) = doc.table("PROJCOST") {
        for r in t.rows.iter().filter(|r| num(t.value(r, "proj_id")) == pid) {
            out.entry(num(t.value(r, "task_id")))
                .or_default()
                .expense_cost += float(t.value(r, "act_cost"));
        }
    }
    out
}

/// Store Period Performance: write into `period` what each activity and
/// assignment of project `pid` earned that no other period of the calendar
/// holds yet, so the stored periods add up to the actuals to date. A later
/// period that already holds performance is counted too (it is not counted
/// twice when an earlier period is stored late). Rows already stored in the
/// period are replaced. `current` is the document with the schedule written
/// in (the project's updated document). Actual this period starts again from
/// zero on the project's activities and assignments. Returns the activities
/// stored.
pub fn store_period(
    doc: &mut Document,
    current: &Document,
    pid: i64,
    calendar: i64,
    period: i64,
) -> Result<usize> {
    let all = periods(doc, calendar);
    let Some(target) = all.iter().find(|p| p.id == period).cloned() else {
        return Err("Choose a financial period.".into());
    };
    let others: Vec<i64> = all
        .iter()
        .filter(|p| p.id != period)
        .map(|p| p.id)
        .collect();
    let before = stored_before(doc, pid, &others);
    let to_date = actual_to_date(current, pid);
    let assignments_before = assignment_before(doc, pid, &others);
    // Replace what the period held for this project.
    for table in ["TASKFIN", "TRSRCFIN"] {
        if let Some(t) = doc
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case(table))
        {
            let (f, p) = (t.field("fin_dates_id"), t.field("proj_id"));
            t.rows.retain(|r| {
                let get = |k: Option<usize>| k.and_then(|k| r.get(k)).map_or(0, |v| num(v));
                !(get(f) == period && get(p) == pid)
            });
        }
    }
    let text = |v: f64| format!("{v:.4}");
    let mut stored = 0;
    for (task, now) in &to_date {
        let was = before.get(task).cloned().unwrap_or_default();
        let delta = Stored {
            labor_units: now.labor_units - was.labor_units,
            nonlabor_units: now.nonlabor_units - was.nonlabor_units,
            labor_cost: now.labor_cost - was.labor_cost,
            nonlabor_cost: now.nonlabor_cost - was.nonlabor_cost,
            material_cost: now.material_cost - was.material_cost,
            expense_cost: now.expense_cost - was.expense_cost,
        };
        let empty = [
            delta.labor_units,
            delta.nonlabor_units,
            delta.labor_cost,
            delta.nonlabor_cost,
            delta.material_cost,
            delta.expense_cost,
        ]
        .iter()
        .all(|v| v.abs() < 1e-9);
        if empty {
            continue;
        }
        records::add_row(
            doc,
            "TASKFIN",
            &[
                ("task_id", task.to_string()),
                ("fin_dates_id", period.to_string()),
                ("proj_id", pid.to_string()),
                ("act_work_qty", text(delta.labor_units)),
                ("act_equip_qty", text(delta.nonlabor_units)),
                ("act_mat_cost", text(delta.material_cost)),
                ("act_work_cost", text(delta.labor_cost)),
                ("act_equip_cost", text(delta.nonlabor_cost)),
                ("act_expense_cost", text(delta.expense_cost)),
            ],
        );
        stored += 1;
    }
    if let Some(t) = current.table("TASKRSRC") {
        let rows: Vec<(i64, i64, f64, f64)> = t
            .rows
            .iter()
            .filter(|r| num(t.value(r, "proj_id")) == pid)
            .map(|r| {
                (
                    num(t.value(r, "taskrsrc_id")),
                    num(t.value(r, "task_id")),
                    float(t.value(r, "act_reg_qty")) + float(t.value(r, "act_ot_qty")),
                    float(t.value(r, "act_reg_cost")) + float(t.value(r, "act_ot_cost")),
                )
            })
            .collect();
        for (id, task, qty, cost) in rows {
            let (q0, c0) = assignments_before.get(&id).copied().unwrap_or_default();
            if (qty - q0).abs() < 1e-9 && (cost - c0).abs() < 1e-9 {
                continue;
            }
            records::add_row(
                doc,
                "TRSRCFIN",
                &[
                    ("taskrsrc_id", id.to_string()),
                    ("fin_dates_id", period.to_string()),
                    ("task_id", task.to_string()),
                    ("proj_id", pid.to_string()),
                    ("act_qty", text(qty - q0)),
                    ("act_cost", text(cost - c0)),
                ],
            );
        }
    }
    // What the period earned is stored; the open period starts from zero.
    openxer_xer::assignments::reset_this_period(doc, pid);
    if let Some(i) = doc.table("PROJECT").and_then(|t| {
        t.rows
            .iter()
            .position(|r| num(t.value(r, "proj_id")) == pid)
    }) {
        // The last stored period stays the latest one.
        let last = num(doc
            .table("PROJECT")
            .map_or("", |t| t.value(&t.rows[i], "last_fin_dates_id")));
        let later = all
            .iter()
            .find(|p| p.id == last)
            .is_some_and(|p| p.start > target.start);
        if !later {
            doc.table_mut("PROJECT")
                .set(i, "last_fin_dates_id", period.to_string());
        }
    }
    Ok(stored)
}

/// The last of `periods` (sorted by start) that ends before `day`: the
/// period Store Period Performance offers first.
pub fn period_before(periods: &[Period], day: NaiveDate) -> Option<i64> {
    periods.iter().rev().find(|p| p.end < day).map(|p| p.id)
}

/// The period of the calendar that contains the day, if any.
pub fn period_containing(doc: &Document, calendar: i64, day: NaiveDate) -> Option<i64> {
    periods(doc, calendar)
        .into_iter()
        .find(|p| p.start <= day && day <= p.end)
        .map(|p| p.id)
}

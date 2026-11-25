//! Resource dictionary edits for the Resources view: add, copy and paste,
//! delete, the Details fields, the Units & Prices rows (RSRCRATE), roles
//! (RSRCROLE) and resource codes (RSRCRCAT). Everything here works on the
//! retained XER tables, so an edit survives save, reopen and export, and the
//! next F9 or Level reads it through the leveling inputs that scheduling
//! rebuilds from the file's rows.
use crate::records::{self, num};
use openxer_xer::{parse_absolute, Document, Table};

/// Resource types: code and caption.
pub const TYPES: [(&str, &str); 3] = [
    ("RT_Labor", "Labor"),
    ("RT_Equip", "Nonlabor"),
    ("RT_Mat", "Material"),
];

/// One copied resource with its rate, role and code rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceClip {
    pub resource: Vec<(String, String)>,
    pub rates: Vec<Vec<(String, String)>>,
    pub roles: Vec<Vec<(String, String)>>,
    pub codes: Vec<Vec<(String, String)>>,
}

fn row_pairs(t: &Table, r: &[String]) -> Vec<(String, String)> {
    t.fields
        .iter()
        .map(|f| (f.clone(), t.value(r, f).to_string()))
        .collect()
}

fn rows_of(doc: &Document, table: &str, rsrc: i64) -> Vec<Vec<(String, String)>> {
    doc.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "rsrc_id")) == rsrc)
            .map(|r| row_pairs(t, r))
            .collect()
    })
}

/// The RSRC row index of a resource.
pub fn resource_row(doc: &Document, rsrc: i64) -> Option<usize> {
    let t = doc.table("RSRC")?;
    t.rows
        .iter()
        .position(|r| num(t.value(r, "rsrc_id")) == rsrc)
}

pub fn value(doc: &Document, rsrc: i64, field: &str) -> String {
    resource_row(doc, rsrc)
        .map(|i| records::get(doc, "RSRC", i, field))
        .unwrap_or_default()
}

/// Set one RSRC column; false when the resource is gone or the value is the same.
pub fn set_value(doc: &mut Document, rsrc: i64, field: &str, value: String) -> bool {
    let Some(i) = resource_row(doc, rsrc) else {
        return false;
    };
    if records::get(doc, "RSRC", i, field) == value {
        return false;
    }
    records::set(doc, "RSRC", i, field, value);
    true
}

fn taken(doc: &Document, code: &str) -> bool {
    doc.table("RSRC").is_some_and(|t| {
        t.rows
            .iter()
            .any(|r| t.value(r, "rsrc_short_name").eq_ignore_ascii_case(code))
    })
}

/// `base`, or `base-1`, `base-2`... when the ID is in use.
pub fn unique_code(doc: &Document, base: &str) -> String {
    if !taken(doc, base) {
        return base.to_string();
    }
    (1..)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !taken(doc, c))
        .expect("an unused ID")
}

/// Resource ID a new resource is given, as observed for Add (with and
/// without the wizard); `R-1`, `R-2`... when it is in use (`unique_code`).
pub const NEW_RESOURCE_ID: &str = "R";
/// Resource name a new resource is given, as observed for Add.
pub const NEW_RESOURCE_NAME: &str = "(New Resource)";

/// Add a resource under `parent` with the defaults of a new resource: ID
/// `NEW_RESOURCE_ID` (or the next free one), name `NEW_RESOURCE_NAME`,
/// labor, the project calendar, 8h/d default and max units, no price, one
/// rate row effective from `effective` (an XER date). Returns its id.
pub fn add_resource(
    doc: &mut Document,
    parent: Option<i64>,
    calendar: &str,
    currency: &str,
    effective: &str,
) -> i64 {
    let id = records::next_id(doc, "RSRC", "rsrc_id");
    let sequence = doc.table("RSRC").map_or(0, |t| {
        t.rows
            .iter()
            .map(|r| num(t.value(r, "rsrc_seq_num")))
            .max()
            .unwrap_or(0)
    }) + 1;
    let code = unique_code(doc, NEW_RESOURCE_ID);
    records::add_row(
        doc,
        "RSRC",
        &[
            ("rsrc_id", id.to_string()),
            (
                "parent_rsrc_id",
                parent.map_or(String::new(), |p| p.to_string()),
            ),
            ("clndr_id", calendar.to_string()),
            ("guid", openxer_xer::new_guid(&format!("rsrc/{id}"))),
            ("rsrc_seq_num", sequence.to_string()),
            ("rsrc_name", NEW_RESOURCE_NAME.into()),
            ("rsrc_short_name", code),
            ("def_qty_per_hr", "1".into()),
            ("cost_qty_type", "QT_Hour".into()),
            ("active_flag", "Y".into()),
            ("auto_compute_act_flag", "N".into()),
            ("def_cost_qty_link_flag", "Y".into()),
            ("ot_flag", "N".into()),
            ("curr_id", currency.to_string()),
            ("rsrc_type", "RT_Labor".into()),
            ("load_tasks_flag", "N".into()),
        ],
    );
    add_rate_row(doc, id, effective, "1", "0");
    id
}

fn add_rate_row(doc: &mut Document, rsrc: i64, date: &str, max: &str, price: &str) -> usize {
    let id = records::next_id(doc, "RSRCRATE", "rsrc_rate_id");
    records::add_row(
        doc,
        "RSRCRATE",
        &[
            ("rsrc_rate_id", id.to_string()),
            ("rsrc_id", rsrc.to_string()),
            ("max_qty_per_hr", max.to_string()),
            ("cost_per_qty", price.to_string()),
            ("start_date", date.to_string()),
            ("cost_per_qty2", "0.0000".into()),
            ("cost_per_qty3", "0.0000".into()),
            ("cost_per_qty4", "0.0000".into()),
            ("cost_per_qty5", "0.0000".into()),
        ],
    )
}

pub fn copy_resource(doc: &Document, rsrc: i64) -> Option<ResourceClip> {
    let t = doc.table("RSRC")?;
    let r = &t.rows[resource_row(doc, rsrc)?];
    Some(ResourceClip {
        resource: row_pairs(t, r),
        rates: rows_of(doc, "RSRCRATE", rsrc),
        roles: rows_of(doc, "RSRCROLE", rsrc),
        codes: rows_of(doc, "RSRCRCAT", rsrc),
    })
}

/// Paste a copied resource under `parent` with new ids, a unique Resource
/// ID and its rates, roles and codes. Returns the new id.
pub fn paste_resource(doc: &mut Document, clip: &ResourceClip, parent: Option<i64>) -> i64 {
    let id = records::next_id(doc, "RSRC", "rsrc_id");
    let get = |k: &str| {
        clip.resource
            .iter()
            .find(|(f, _)| f == k)
            .map_or(String::new(), |(_, v)| v.clone())
    };
    let code = unique_code(doc, &get("rsrc_short_name"));
    let sequence = doc.table("RSRC").map_or(0, |t| {
        t.rows
            .iter()
            .map(|r| num(t.value(r, "rsrc_seq_num")))
            .max()
            .unwrap_or(0)
    }) + 1;
    let values: Vec<(&str, String)> = clip
        .resource
        .iter()
        .map(|(f, v)| {
            let v = match f.as_str() {
                "rsrc_id" => id.to_string(),
                "parent_rsrc_id" => parent.map_or(String::new(), |p| p.to_string()),
                "rsrc_short_name" => code.clone(),
                "guid" => openxer_xer::new_guid(&format!("rsrc/{id}")),
                "rsrc_seq_num" => sequence.to_string(),
                "user_id" | "last_checksum" => String::new(),
                _ => v.clone(),
            };
            (f.as_str(), v)
        })
        .collect();
    records::add_row(doc, "RSRC", &values);
    for (table, key) in [
        ("RSRCRATE", Some("rsrc_rate_id")),
        ("RSRCROLE", None),
        ("RSRCRCAT", None),
    ] {
        let rows = match table {
            "RSRCRATE" => &clip.rates,
            "RSRCROLE" => &clip.roles,
            _ => &clip.codes,
        };
        for row in rows {
            let next = key.map(|k| records::next_id(doc, table, k));
            let values: Vec<(&str, String)> = row
                .iter()
                .map(|(f, v)| {
                    let v = if f == "rsrc_id" {
                        id.to_string()
                    } else if Some(f.as_str()) == key {
                        next.unwrap_or(0).to_string()
                    } else {
                        v.clone()
                    };
                    (f.as_str(), v)
                })
                .collect();
            records::add_row(doc, table, &values);
        }
    }
    id
}

/// Paste copied resources under `parent`, keeping their hierarchy: a
/// resource whose parent was copied with it goes under that parent's copy;
/// the others go under `parent`. Returns the new ids in clipboard order.
pub fn paste_resources(
    doc: &mut Document,
    clips: &[ResourceClip],
    parent: Option<i64>,
) -> Vec<i64> {
    let field = |clip: &ResourceClip, k: &str| {
        clip.resource
            .iter()
            .find(|(f, _)| f == k)
            .map_or(0, |(_, v)| num(v))
    };
    let added: Vec<i64> = clips
        .iter()
        .map(|clip| paste_resource(doc, clip, parent))
        .collect();
    let copies: std::collections::BTreeMap<i64, i64> = clips
        .iter()
        .zip(&added)
        .map(|(clip, id)| (field(clip, "rsrc_id"), *id))
        .collect();
    for (clip, id) in clips.iter().zip(&added) {
        let old_parent = field(clip, "parent_rsrc_id");
        if let Some(new_parent) = copies.get(&old_parent).filter(|_| old_parent != 0) {
            set_value(doc, *id, "parent_rsrc_id", new_parent.to_string());
        }
    }
    added
}

/// The resources of `ids` that have no ancestor in `ids` (the top-most
/// nodes of a selection), in the order given. Moving these moves the rest
/// with them.
pub fn top_most(doc: &Document, ids: &[i64]) -> Vec<i64> {
    let set: std::collections::BTreeSet<i64> = ids.iter().copied().collect();
    ids.iter()
        .copied()
        .filter(|&id| {
            let mut up = Some(num(&value(doc, id, "parent_rsrc_id"))).filter(|p| *p != 0);
            let mut steps = 0;
            while let Some(p) = up {
                if set.contains(&p) {
                    return false;
                }
                steps += 1;
                if steps > 100_000 || p == id {
                    break;
                }
                up = Some(num(&value(doc, p, "parent_rsrc_id"))).filter(|q| *q != 0);
            }
            true
        })
        .collect()
}

/// Move a resource under `parent` (`None`: the top), last among its new
/// siblings. A resource cannot go under itself or one of its own children.
pub fn move_resource(doc: &mut Document, rsrc: i64, parent: Option<i64>) -> Result<(), String> {
    let row = resource_row(doc, rsrc).ok_or("The resource no longer exists")?;
    let code = records::get(doc, "RSRC", row, "rsrc_short_name");
    // Walk up from the new parent; meeting the resource means a cycle.
    let mut up = parent;
    let mut steps = 0;
    while let Some(id) = up {
        if id == rsrc {
            return Err(format!(
                "Resource {code} cannot be pasted under itself or one of its children."
            ));
        }
        steps += 1;
        if steps > 100_000 {
            break;
        }
        up = Some(num(&value(doc, id, "parent_rsrc_id"))).filter(|p| *p != 0);
    }
    let current = Some(num(&records::get(doc, "RSRC", row, "parent_rsrc_id"))).filter(|p| *p != 0);
    if current == parent {
        return Ok(());
    }
    let sequence = doc.table("RSRC").map_or(0, |t| {
        t.rows
            .iter()
            .map(|r| num(t.value(r, "rsrc_seq_num")))
            .max()
            .unwrap_or(0)
    }) + 1;
    records::set(
        doc,
        "RSRC",
        row,
        "parent_rsrc_id",
        parent.map_or(String::new(), |p| p.to_string()),
    );
    records::set(doc, "RSRC", row, "rsrc_seq_num", sequence.to_string());
    Ok(())
}

/// Delete a resource with its rates, roles, codes, leveling list entries
/// and user defined field values (the shared edit layer). A resource that
/// is assigned or has child resources is refused with the reason.
pub fn delete_resource(doc: &mut Document, rsrc: i64) -> Result<(), String> {
    openxer_xer::edit::resources::delete(doc, rsrc)
}

// ------------------------------------------------------------------ rates

/// RSRCRATE row indices of a resource, by effective date.
pub fn rate_rows(doc: &Document, rsrc: i64) -> Vec<usize> {
    let Some(t) = doc.table("RSRCRATE") else {
        return vec![];
    };
    let mut rows: Vec<usize> = (0..t.rows.len())
        .filter(|&i| num(t.value(&t.rows[i], "rsrc_id")) == rsrc)
        .collect();
    rows.sort_by_key(|&i| parse_absolute(t.value(&t.rows[i], "start_date")));
    rows
}

/// Add a rate row a day after the latest one (or on `fallback`), carrying
/// its max units and prices. Returns the row.
pub fn add_rate(doc: &mut Document, rsrc: i64, fallback: &str) -> usize {
    let rows = rate_rows(doc, rsrc);
    let Some(&last) = rows.last() else {
        return add_rate_row(doc, rsrc, fallback, "1", "0");
    };
    let get = |doc: &Document, k: &str| records::get(doc, "RSRCRATE", last, k);
    let date = parse_absolute(&get(doc, "start_date"))
        .map(|d| openxer_xer::date_string(0, Some(d + openxer_engine::calendar::DAY)))
        .unwrap_or_else(|| fallback.to_string());
    let (max, price) = (get(doc, "max_qty_per_hr"), get(doc, "cost_per_qty"));
    let row = add_rate_row(doc, rsrc, &date, &max, &price);
    for k in [
        "cost_per_qty2",
        "cost_per_qty3",
        "cost_per_qty4",
        "cost_per_qty5",
    ] {
        let v = get(doc, k);
        records::set(doc, "RSRCRATE", row, k, v);
    }
    row
}

/// Delete a rate row; a resource keeps at least one.
pub fn delete_rate(doc: &mut Document, row: usize) -> Result<(), String> {
    let rsrc = num(&records::get(doc, "RSRCRATE", row, "rsrc_id"));
    if rate_rows(doc, rsrc).len() <= 1 {
        return Err("A resource needs at least one price and limit row.".into());
    }
    records::remove_row(doc, "RSRCRATE", row);
    Ok(())
}

/// Set a rate row's effective date (absolute minutes); dates are unique per
/// resource.
pub fn set_rate_date(doc: &mut Document, row: usize, date: i64) -> Result<(), String> {
    let rsrc = num(&records::get(doc, "RSRCRATE", row, "rsrc_id"));
    let clash = rate_rows(doc, rsrc).into_iter().any(|r| {
        r != row && parse_absolute(&records::get(doc, "RSRCRATE", r, "start_date")) == Some(date)
    });
    if clash {
        return Err("Another row already starts on that date.".into());
    }
    records::set(
        doc,
        "RSRCRATE",
        row,
        "start_date",
        openxer_xer::date_string(0, Some(date)),
    );
    Ok(())
}

/// Set max units per hour or a price column of a rate row.
pub fn set_rate_number(doc: &mut Document, row: usize, field: &str, v: f64) -> Result<(), String> {
    if !(0. ..1.0e12).contains(&v) {
        return Err("Enter a value of zero or more.".into());
    }
    let text = if field == "max_qty_per_hr" {
        openxer_xer::assignments::quantity_text(v)
    } else {
        openxer_xer::assignments::cost_text(v)
    };
    records::set(doc, "RSRCRATE", row, field, text);
    Ok(())
}

// ------------------------------------------------------------------ roles

/// Give the resource a role (proficiency 3); the first role becomes primary.
pub fn assign_role(doc: &mut Document, rsrc: i64, role: i64) -> Result<(), String> {
    let has = doc.table("RSRCROLE").is_some_and(|t| {
        t.rows
            .iter()
            .any(|r| num(t.value(r, "rsrc_id")) == rsrc && num(t.value(r, "role_id")) == role)
    });
    if has {
        return Err("The resource already has that role.".into());
    }
    records::add_row(
        doc,
        "RSRCROLE",
        &[
            ("rsrc_id", rsrc.to_string()),
            ("role_id", role.to_string()),
            ("skill_level", "3".into()),
        ],
    );
    if value(doc, rsrc, "role_id").trim().is_empty() {
        set_value(doc, rsrc, "role_id", role.to_string());
    }
    Ok(())
}

pub fn remove_role(doc: &mut Document, rsrc: i64, role: i64) {
    if let Some(t) = doc.tables.iter_mut().find(|t| t.name == "RSRCROLE") {
        let (a, b) = (t.field("rsrc_id"), t.field("role_id"));
        if let (Some(a), Some(b)) = (a, b) {
            t.rows
                .retain(|r| !(num(&r[a]) == rsrc && r.get(b).is_some_and(|x| num(x) == role)));
        }
    }
    if num(&value(doc, rsrc, "role_id")) == role {
        set_value(doc, rsrc, "role_id", String::new());
    }
}

// ------------------------------------------------------------------ codes

/// Resource code values: (value id, type id, type name, value, description).
pub fn code_values(doc: &Document) -> Vec<(i64, i64, String, String, String)> {
    let types: Vec<(i64, String)> = doc.table("RCATTYPE").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    num(t.value(r, "rsrc_catg_type_id")),
                    t.value(r, "rsrc_catg_type").to_string(),
                )
            })
            .collect()
    });
    doc.table("RCATVAL").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                let kind = num(t.value(r, "rsrc_catg_type_id"));
                (
                    num(t.value(r, "rsrc_catg_id")),
                    kind,
                    types
                        .iter()
                        .find(|(k, _)| *k == kind)
                        .map_or(String::new(), |(_, n)| n.clone()),
                    t.value(r, "rsrc_catg_short_name").to_string(),
                    t.value(r, "rsrc_catg_name").to_string(),
                )
            })
            .collect()
    })
}

/// Assign a code value; it replaces the resource's value of the same type.
pub fn assign_code(doc: &mut Document, rsrc: i64, value_id: i64) -> Result<(), String> {
    let (_, kind, ..) = code_values(doc)
        .into_iter()
        .find(|v| v.0 == value_id)
        .ok_or("The code value is not in this file.")?;
    remove_code(doc, rsrc, kind);
    records::add_row(
        doc,
        "RSRCRCAT",
        &[
            ("rsrc_id", rsrc.to_string()),
            ("rsrc_catg_type_id", kind.to_string()),
            ("rsrc_catg_id", value_id.to_string()),
        ],
    );
    Ok(())
}

/// Remove the resource's value of a code type.
pub fn remove_code(doc: &mut Document, rsrc: i64, kind: i64) {
    if let Some(t) = doc.tables.iter_mut().find(|t| t.name == "RSRCRCAT") {
        let (a, b) = (t.field("rsrc_id"), t.field("rsrc_catg_type_id"));
        if let (Some(a), Some(b)) = (a, b) {
            t.rows
                .retain(|r| !(num(&r[a]) == rsrc && r.get(b).is_some_and(|x| num(x) == kind)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxer_xer::ProjectData;

    fn project() -> ProjectData {
        super::super::resources::tests::with_resource()
    }

    #[test]
    fn add_copy_paste_and_delete_round_trip_through_xer() {
        let mut p = project();
        let doc = &mut p.document;
        let id = add_resource(doc, Some(7), "1", "", "2026-11-02 00:00");
        assert_eq!(value(doc, id, "rsrc_short_name"), "R");
        assert_eq!(value(doc, id, "rsrc_name"), "(New Resource)");
        assert_eq!(value(doc, id, "parent_rsrc_id"), "7");
        assert_eq!(rate_rows(doc, id).len(), 1);
        let second = add_resource(doc, None, "1", "", "2026-11-02 00:00");
        assert_eq!(value(doc, second, "rsrc_short_name"), "R-1");
        // Copy PIPE (two rates) and paste it at the top level.
        let clip = copy_resource(doc, 7).unwrap();
        let pasted = paste_resource(doc, &clip, None);
        assert_eq!(value(doc, pasted, "rsrc_short_name"), "PIPE-1");
        assert_eq!(value(doc, pasted, "rsrc_name"), "Pipefitter");
        assert_eq!(value(doc, pasted, "parent_rsrc_id"), "");
        assert_eq!(rate_rows(doc, pasted).len(), 2);
        // The edits survive an export and reimport.
        let text = p.updated_document().text();
        let back = Document::parse(&text).unwrap();
        assert_eq!(value(&back, pasted, "rsrc_short_name"), "PIPE-1");
        assert_eq!(rate_rows(&back, pasted).len(), 2);
        assert_eq!(value(&back, id, "def_qty_per_hr"), "1");
        // An assigned resource is refused; an unassigned one goes with its rows.
        let doc = &mut p.document;
        let error = delete_resource(doc, 7).unwrap_err();
        assert!(error.contains("1 activity assignment"), "{error}");
        delete_resource(doc, pasted).unwrap();
        assert!(resource_row(doc, pasted).is_none());
        assert!(rate_rows(doc, pasted).is_empty());
        // A parent is refused while it has children.
        assert!(delete_resource(doc, 8).is_ok());
        let child = add_resource(doc, Some(9), "1", "", "2026-11-02 00:00");
        assert!(delete_resource(doc, 9).unwrap_err().contains("child"));
        delete_resource(doc, child).unwrap();
        delete_resource(doc, 9).unwrap();
    }

    #[test]
    fn rate_rows_add_date_and_delete() {
        let mut p = project();
        let doc = &mut p.document;
        let row = add_rate(doc, 7, "2026-11-02 00:00");
        assert_eq!(
            records::get(doc, "RSRCRATE", row, "start_date"),
            "2026-10-02 00:00"
        );
        assert_eq!(records::get(doc, "RSRCRATE", row, "max_qty_per_hr"), "2");
        let rows = rate_rows(doc, 7);
        assert_eq!(rows.len(), 3);
        let first = parse_absolute("2026-01-01 00:00").unwrap();
        assert!(set_rate_date(doc, row, first).is_err(), "dates are unique");
        set_rate_number(doc, row, "max_qty_per_hr", 3.).unwrap();
        set_rate_number(doc, row, "cost_per_qty", 60.).unwrap();
        assert_eq!(
            records::get(doc, "RSRCRATE", row, "cost_per_qty"),
            "60.0000"
        );
        assert!(set_rate_number(doc, row, "cost_per_qty", -1.).is_err());
        for r in rate_rows(doc, 7).into_iter().rev().skip(1) {
            delete_rate(doc, r).unwrap();
        }
        let last = rate_rows(doc, 7)[0];
        assert!(delete_rate(doc, last).is_err(), "one row stays");
    }

    #[test]
    fn roles_and_codes_attach_and_detach() {
        let mut p = project();
        let doc = &mut p.document;
        doc.table_mut("ROLES").add(&[
            ("role_id", "3".into()),
            ("role_short_name", "PF".into()),
            ("role_name", "Pipefitter".into()),
        ]);
        assign_role(doc, 9, 3).unwrap();
        assert!(assign_role(doc, 9, 3).is_err());
        assert_eq!(value(doc, 9, "role_id"), "3", "first role is primary");
        remove_role(doc, 9, 3);
        assert_eq!(value(doc, 9, "role_id"), "");
        assert!(doc.table("RSRCROLE").unwrap().rows.is_empty());
        doc.table_mut("RCATTYPE").add(&[
            ("rsrc_catg_type_id", "1".into()),
            ("rsrc_catg_type", "Craft".into()),
        ]);
        for (id, short) in [("11", "PF"), ("12", "EL")] {
            doc.table_mut("RCATVAL").add(&[
                ("rsrc_catg_id", id.into()),
                ("rsrc_catg_type_id", "1".into()),
                ("rsrc_catg_short_name", short.into()),
            ]);
        }
        assign_code(doc, 9, 11).unwrap();
        assign_code(doc, 9, 12).unwrap();
        let t = doc.table("RSRCRCAT").unwrap();
        assert_eq!(t.rows.len(), 1, "one value per code type");
        assert_eq!(t.value(&t.rows[0], "rsrc_catg_id"), "12");
        remove_code(doc, 9, 1);
        assert!(doc.table("RSRCRCAT").unwrap().rows.is_empty());
    }
}

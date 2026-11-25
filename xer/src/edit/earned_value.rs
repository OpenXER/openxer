//! Earned value techniques of a WBS element (or of the project node, which
//! every element without its own settings follows): how performance percent
//! complete is measured (`PROJWBS.ev_compute_type`, with the custom percent
//! in `ev_user_pct`) and how the estimate to complete is computed
//! (`ev_etc_compute_type`, with the performance factor in
//! `ev_etc_user_value`). The WBS view's Earned Value tab, the command line
//! and the MCP server all set them through these functions; the earned
//! value columns read the same fields. The fields travel in XER with the
//! WBS rows.
use super::records::{self, float, num};
use crate::ProjectData;

pub const TABLE: &str = "PROJWBS";

type R<T> = Result<T, String>;

/// Performance percent complete techniques: (code, operation name,
/// caption). The first is the default.
pub const PERFORMANCE: [(&str, &str, &str); 5] = [
    (
        "EC_Cmp_pct",
        "activity_percent",
        "Activity percent complete",
    ),
    (
        "EC_WBS_pct",
        "wbs_milestones",
        "WBS Milestones percent complete",
    ),
    ("EC_0_100", "0/100", "0/100"),
    ("EC_50_50", "50/50", "50/50"),
    ("EC_User_pct", "custom", "Custom percent complete"),
];

/// Estimate to complete techniques: (code, operation name, caption). The
/// first is the default.
pub const TO_COMPLETE: [(&str, &str, &str); 5] = [
    (
        "EE_Rem_hr",
        "remaining",
        "ETC = remaining cost for activity",
    ),
    ("EE_PF_1", "pf_1", "PF = 1"),
    ("EE_PF_cpi", "pf_cpi", "PF = 1 / Cost Performance Index"),
    (
        "EE_PF_cpi_spi",
        "pf_cpi_spi",
        "PF = 1 / (Cost Performance Index * Schedule Performance Index)",
    ),
    ("EE_PF_usr", "pf", "PF ="),
];

/// The stored techniques of one element, defaults filled in.
#[derive(Clone, Debug, PartialEq)]
pub struct Techniques {
    /// A `PERFORMANCE` code.
    pub performance: String,
    /// The custom percent complete, 0 to 100.
    pub custom_percent: f64,
    /// A `TO_COMPLETE` code.
    pub to_complete: String,
    /// The performance factor of `PF =`.
    pub factor: f64,
}

fn pick(
    list: &[(&'static str, &'static str, &'static str)],
    text: &str,
    what: &str,
) -> R<&'static str> {
    let t = text.trim();
    list.iter()
        .find(|(code, name, caption)| {
            code.eq_ignore_ascii_case(t)
                || name.eq_ignore_ascii_case(t)
                || caption.eq_ignore_ascii_case(t)
        })
        .map(|(code, _, _)| *code)
        .ok_or_else(|| {
            let names: Vec<&str> = list.iter().map(|(_, n, _)| *n).collect();
            format!("Unknown {what} {t:?}; use {}.", names.join(", "))
        })
}

/// A performance technique from its code, operation name or caption.
pub fn performance_code(text: &str) -> R<&'static str> {
    pick(&PERFORMANCE, text, "performance percent complete technique")
}

/// An estimate to complete technique from its code, operation name or caption.
pub fn to_complete_code(text: &str) -> R<&'static str> {
    pick(&TO_COMPLETE, text, "estimate to complete technique")
}

/// A custom percent complete: 0 to 100, with or without `%`.
pub fn parse_percent(text: &str) -> R<f64> {
    let t = text.trim().trim_end_matches('%').trim();
    let v = t
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{t:?} is not a percent such as 25."))?;
    if !(0. ..=100.).contains(&v) {
        return Err("The custom percent complete is from 0 to 100.".into());
    }
    Ok(v)
}

/// A performance factor: a number above 0 (at most 100).
pub fn parse_factor(text: &str) -> R<f64> {
    let t = text.trim();
    let v = t
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{t:?} is not a performance factor such as 1.2."))?;
    if v <= 0. || v > 100. {
        return Err("The performance factor is a number above 0 and at most 100.".into());
    }
    Ok(v)
}

fn row(p: &ProjectData, wbs_id: i64) -> R<usize> {
    let pid = p.model.project.proj_id;
    p.document
        .table(TABLE)
        .and_then(|t| {
            t.rows.iter().position(|r| {
                num(t.value(r, "wbs_id")) == wbs_id && num(t.value(r, "proj_id")) == pid
            })
        })
        .ok_or_else(|| "That WBS element is not in this project.".into())
}

/// The techniques stored on an element (blank fields read as the defaults).
pub fn get(p: &ProjectData, wbs_id: i64) -> R<Techniques> {
    let r = row(p, wbs_id)?;
    let v = |c: &str| records::get(&p.document, TABLE, r, c);
    let or = |s: String, default: &str| {
        if s.trim().is_empty() {
            default.to_string()
        } else {
            s.trim().to_string()
        }
    };
    let factor = v("ev_etc_user_value");
    Ok(Techniques {
        performance: or(v("ev_compute_type"), PERFORMANCE[0].0),
        custom_percent: float(&v("ev_user_pct")),
        to_complete: or(v("ev_etc_compute_type"), TO_COMPLETE[0].0),
        factor: if factor.trim().is_empty() {
            1.
        } else {
            float(&factor)
        },
    })
}

/// The element's id from a WBS code path; the empty path is the project node.
pub fn wbs_id(p: &ProjectData, path: &str) -> R<i64> {
    if path.trim().is_empty() {
        return super::wbs::project_node(p)
            .ok_or_else(|| "The project has no project node.".into());
    }
    super::wbs::find_path(p, path).ok_or_else(|| format!("No WBS element {path:?}."))
}

fn put(p: &mut ProjectData, wbs_id: i64, column: &str, value: String) -> R<bool> {
    let r = row(p, wbs_id)?;
    if records::get(&p.document, TABLE, r, column) == value {
        return Ok(false);
    }
    records::set(&mut p.document, TABLE, r, column, value);
    Ok(true)
}

/// Set the performance percent complete technique. Returns whether it changed.
pub fn set_performance(p: &mut ProjectData, wbs_id: i64, technique: &str) -> R<bool> {
    let code = performance_code(technique)?;
    put(p, wbs_id, "ev_compute_type", code.into())
}

/// Set the custom percent complete (typed text).
pub fn set_custom_percent(p: &mut ProjectData, wbs_id: i64, text: &str) -> R<bool> {
    let v = parse_percent(text)?;
    put(p, wbs_id, "ev_user_pct", records::format_number(v))
}

/// Set the estimate to complete technique.
pub fn set_to_complete(p: &mut ProjectData, wbs_id: i64, technique: &str) -> R<bool> {
    let code = to_complete_code(technique)?;
    put(p, wbs_id, "ev_etc_compute_type", code.into())
}

/// Set the performance factor of `PF =` (typed text).
pub fn set_factor(p: &mut ProjectData, wbs_id: i64, text: &str) -> R<bool> {
    let v = parse_factor(text)?;
    put(p, wbs_id, "ev_etc_user_value", records::format_number(v))
}

/// Set any of the four by name, validated, all or nothing.
pub fn set(
    p: &mut ProjectData,
    wbs_id: i64,
    performance: Option<&str>,
    custom_percent: Option<&str>,
    to_complete: Option<&str>,
    factor: Option<&str>,
) -> R<bool> {
    if performance.is_none()
        && custom_percent.is_none()
        && to_complete.is_none()
        && factor.is_none()
    {
        return Err(
            "Name at least one of performance, custom_percent, estimate_to_complete or performance_factor."
                .into(),
        );
    }
    row(p, wbs_id)?;
    // Validate everything before writing anything.
    if let Some(t) = performance {
        performance_code(t)?;
    }
    if let Some(t) = custom_percent {
        parse_percent(t)?;
    }
    if let Some(t) = to_complete {
        to_complete_code(t)?;
    }
    if let Some(t) = factor {
        parse_factor(t)?;
    }
    let mut changed = false;
    if let Some(t) = performance {
        changed |= set_performance(p, wbs_id, t)?;
    }
    if let Some(t) = custom_percent {
        changed |= set_custom_percent(p, wbs_id, t)?;
    }
    if let Some(t) = to_complete {
        changed |= set_to_complete(p, wbs_id, t)?;
    }
    if let Some(t) = factor {
        changed |= set_factor(p, wbs_id, t)?;
    }
    Ok(changed)
}

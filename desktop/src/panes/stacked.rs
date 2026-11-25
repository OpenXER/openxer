//! Stacked Histogram: the usage profiles' bars split by resource or by
//! role. Each period's bar stacks the units (or costs) of every resource,
//! or every role, that works in it: actual plus remaining, spread the same
//! way as the profiles' own bars (the `views::assignments` spreads, which
//! follow resource curves), so a stack is as tall as the
//! unstacked total. Series are ordered by their total, largest at the
//! bottom; beyond the palette's colors the rest are summed into "Other".
//!
//! With Show Actual and Remaining on as well, each series' segment is split
//! in two: its actual units at the bottom in a darker shade of the series
//! color, its remaining units above in the series color. Off, each segment
//! is one block of actual plus remaining.
use crate::theme;
use crate::views::assignments::{self as asg, Assignment};
use crate::views::resources as rs;
use eframe::egui::Color32;
use openxer_xer::ProjectData;
use std::collections::BTreeMap;

/// What the stacks are split by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StackBy {
    #[default]
    Resource,
    Role,
}

impl StackBy {
    pub fn key(self) -> &'static str {
        match self {
            StackBy::Resource => "resource",
            StackBy::Role => "role",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "resource" => Some(StackBy::Resource),
            "role" => Some(StackBy::Role),
            _ => None,
        }
    }
    /// The legend title.
    pub fn title(self) -> &'static str {
        match self {
            StackBy::Resource => "By Resource",
            StackBy::Role => "By Role",
        }
    }
}

/// One stacked series: its legend label and a value per period.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub label: String,
    /// Actual plus remaining, per period.
    pub values: Vec<f64>,
    /// The actual part of `values`, per period.
    pub actual: Vec<f64>,
    /// The summed rest beyond the palette.
    pub other: bool,
}

/// Fill and outline of series `k`.
pub fn colors(k: usize, other: bool) -> (Color32, Color32) {
    let hex = if other {
        theme::STACK_OTHER
    } else {
        theme::STACK[k % theme::STACK.len()]
    };
    (theme::color(hex), theme::color(theme::shade(hex, 60)))
}

/// Fill and outline of series `k`'s actual part (Show Actual and
/// Remaining): a darker shade of its color.
pub fn actual_colors(k: usize, other: bool) -> (Color32, Color32) {
    let hex = if other {
        theme::STACK_OTHER
    } else {
        theme::STACK[k % theme::STACK.len()]
    };
    (
        theme::color(theme::shade(hex, 68)),
        theme::color(theme::shade(hex, 40)),
    )
}

/// The segments of one period's stack, from the bottom: one per series, or
/// with `split` an actual and a remaining one per series.
pub fn segments(series: &[Series], n: usize, split: bool) -> Vec<(f64, f64, (Color32, Color32))> {
    let mut base = 0.;
    let mut out = vec![];
    for (k, s) in series.iter().enumerate() {
        let v = s.values.get(n).copied().unwrap_or(0.);
        if split {
            let a = s.actual.get(n).copied().unwrap_or(0.).clamp(0., v.max(0.));
            out.push((base, base + a, actual_colors(k, s.other)));
            out.push((base + a, base + v, colors(k, s.other)));
        } else {
            out.push((base, base + v, colors(k, s.other)));
        }
        base += v;
    }
    out
}

/// Units (costs when `costs`) per period of `list`, split by resource or
/// role.
pub fn series(
    p: &ProjectData,
    list: &[&Assignment],
    periods: &[(i64, i64)],
    costs: bool,
    by: StackBy,
) -> Vec<Series> {
    let n = periods.len();
    let m = &p.model;
    let codes: BTreeMap<i64, String> = rs::resources(p)
        .into_iter()
        .map(|r| (r.id, r.code))
        .collect();
    let roles = rs::role_names(p);
    let mut sums: BTreeMap<String, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for x in list {
        let costed;
        let x = if costs {
            costed = x.as_costs();
            &costed
        } else {
            *x
        };
        let label = match by {
            StackBy::Resource => codes
                .get(&x.rsrc)
                .cloned()
                .unwrap_or_else(|| format!("Resource {}", x.rsrc)),
            StackBy::Role => match x.role.trim() {
                "" => "No role".to_string(),
                id => roles
                    .get(id)
                    .filter(|n| !n.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("Role {id}")),
            },
        };
        let (values, actual) = sums
            .entry(label)
            .or_insert_with(|| (vec![0.; n], vec![0.; n]));
        if m.activities[x.task].act_start.is_some() {
            let spent = asg::actual_spread(p, x, periods);
            for k in 0..n {
                values[k] += spent[k];
                actual[k] += spent[k];
            }
        }
        for (k, v) in asg::remaining_spread(p, x, periods).into_iter().enumerate() {
            values[k] += v;
        }
    }
    let mut out: Vec<(f64, Series)> = sums
        .into_iter()
        .map(|(label, (values, actual))| {
            let total: f64 = values.iter().sum();
            (
                total,
                Series {
                    label,
                    values,
                    actual,
                    other: false,
                },
            )
        })
        .filter(|(total, _)| *total > 1e-9)
        .collect();
    // Largest first; ties by label (BTreeMap order is kept by the stable sort).
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut series: Vec<Series> = out.into_iter().map(|(_, s)| s).collect();
    let room = theme::STACK.len();
    if series.len() > room {
        let rest = series.split_off(room - 1);
        let mut values = vec![0.; n];
        let mut actual = vec![0.; n];
        for s in &rest {
            for (v, x) in values.iter_mut().zip(&s.values) {
                *v += x;
            }
            for (v, x) in actual.iter_mut().zip(&s.actual) {
                *v += x;
            }
        }
        series.push(Series {
            label: format!("Other ({})", rest.len()),
            values,
            actual,
            other: true,
        });
    }
    series
}

/// The assignments a resource profile stacks: every assignment of the
/// resources of the same type as `rsrc` (labor, nonlabor or material; a
/// material's in the same unit of measure only, as other units do not add).
pub fn resource_profile_assignments(p: &ProjectData, rsrc: Option<i64>) -> Vec<Assignment> {
    let list = rs::resources(p);
    let (kind, unit) = rsrc
        .and_then(|id| list.iter().find(|r| r.id == id))
        .map_or((rs::ResourceType::Labor, ""), |r| (r.kind, r.unit.trim()));
    let same: std::collections::BTreeSet<i64> = list
        .iter()
        .filter(|r| r.kind == kind && (kind != rs::ResourceType::Material || r.unit.trim() == unit))
        .map(|r| r.id)
        .collect();
    asg::assignments(p)
        .into_iter()
        .filter(|a| same.contains(&a.rsrc))
        .collect()
}

//! Resource curves (the RSRCCURVDATA table) and OpenXER's preset curves.
//!
//! A curve is 21 points, `pct_usage_0` to `pct_usage_20`: the percent of an
//! assignment's units spent in each 5% step of its duration. Point 0 is the
//! start instant and point k (1 to 20) is the share spent from (k - 1) x 5%
//! to k x 5% of the duration, so the points total 100.
//!
//! The presets are OpenXER's own: each is a density f(t) over the duration
//! t in [0, 1], and point k is 100 x (F(k/20) - F((k-1)/20)) where F is the
//! cumulative share of f (its integral from 0, divided by the integral over
//! [0, 1]). F is integrated numerically (Simpson's rule, 64 slices per 5%
//! step). The points are then rounded to 0.01 so that they total exactly
//! 100: each point is floored to a hundredth and the hundredths left over go
//! to the points with the largest remainders (the earlier point on a tie).
//!
//! Presets are not written into a file until an assignment uses one: the
//! list a user sees is the file's own curves plus every preset whose name
//! the file does not already hold. Picking a preset copies it into the
//! file's RSRCCURVDATA with a new `curv_id` (and `default_flag` N), and the
//! row stays recognised as that preset (same name, same points) after save
//! and reload, so it is never added twice. A file's own curve with a
//! preset's name and different points is the file's and hides the preset.
use super::records::{self, float, num};
use crate::Document;
use std::sync::OnceLock;

pub const TABLE: &str = "RSRCCURVDATA";
/// Points per curve: 0% to 100% of the duration in 5% steps.
pub const POINTS: usize = 21;

/// One preset curve.
#[derive(Clone, Debug)]
pub struct Preset {
    pub name: &'static str,
    /// The shape in plain words.
    pub shape: &'static str,
    /// The formula the points come from.
    pub formula: &'static str,
    pub points: [f64; POINTS],
}

/// A normal density, unscaled (the scale cancels when F is normalised).
fn normal(t: f64, mean: f64, sd: f64) -> f64 {
    let z = (t - mean) / sd;
    (-0.5 * z * z).exp() / sd
}

/// The trapezoid of "Steady with Ramps": up over the first 20%, level, down
/// over the last 20%.
fn trapezoid(t: f64) -> f64 {
    (t / 0.2).min(1.).min((1. - t) / 0.2).max(0.)
}

type Density = fn(f64) -> f64;

/// (name, shape in plain words, formula, density).
const DEFINITIONS: [(&str, &str, &str, Density); 10] = [
    (
        "Even",
        "The same amount in every step.",
        "f(t) = 1 (uniform)",
        |_| 1.,
    ),
    (
        "Early Weighted",
        "Heaviest at the start and falling away; about seven eighths of the units are spent in the first half.",
        "f(t) = (1 - t)^2 (beta distribution, a = 1, b = 3)",
        |t| (1. - t) * (1. - t),
    ),
    (
        "Late Weighted",
        "Light at the start and heaviest at the finish; the mirror of Early Weighted.",
        "f(t) = t^2 (beta distribution, a = 3, b = 1)",
        |t| t * t,
    ),
    (
        "Ramp Up",
        "Rising in a straight line from nothing at the start to the most at the finish.",
        "f(t) = t (triangular, peak at the finish)",
        |t| t,
    ),
    (
        "Ramp Down",
        "Falling in a straight line from the most at the start to nothing at the finish.",
        "f(t) = 1 - t (triangular, peak at the start)",
        |t| 1. - t,
    ),
    (
        "Bell, Centered",
        "Slow start, peak at mid duration, slow finish.",
        "normal distribution, mean 0.5, standard deviation 0.15, truncated to [0, 1]",
        |t| normal(t, 0.5, 0.15),
    ),
    (
        "Bell, Early Peak",
        "A bell that peaks at 30% of the duration.",
        "normal distribution, mean 0.3, standard deviation 0.12, truncated to [0, 1]",
        |t| normal(t, 0.3, 0.12),
    ),
    (
        "Bell, Late Peak",
        "A bell that peaks at 70% of the duration.",
        "normal distribution, mean 0.7, standard deviation 0.12, truncated to [0, 1]",
        |t| normal(t, 0.7, 0.12),
    ),
    (
        "Two Peaks",
        "Two equal peaks at 25% and 75% of the duration with a dip between them.",
        "equal mix of two normal distributions, means 0.25 and 0.75, standard deviation 0.1, truncated to [0, 1]",
        |t| normal(t, 0.25, 0.1) + normal(t, 0.75, 0.1),
    ),
    (
        "Steady with Ramps",
        "Ramps up over the first 20%, holds level, and ramps down over the last 20%.",
        "trapezoid: f(t) = min(t / 0.2, 1, (1 - t) / 0.2)",
        trapezoid,
    ),
];

/// Simpson's rule over [a, b] in `n` (even) slices.
fn simpson(f: Density, a: f64, b: f64, n: usize) -> f64 {
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        let w = if i % 2 == 1 { 4. } else { 2. };
        sum += w * f(a + h * i as f64);
    }
    sum * h / 3.
}

/// Shares (summing to 100) rounded to 0.01 with a total of exactly 100.
pub fn round_to_hundredths(shares: &[f64; POINTS]) -> [f64; POINTS] {
    // A tiny guard keeps exact hundredths (0.25) from flooring below.
    let units: Vec<f64> = shares.iter().map(|s| s * 100. + 1e-9).collect();
    let mut out: Vec<i64> = units.iter().map(|t| t.floor() as i64).collect();
    let short = 10000 - out.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..POINTS).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (units[a] - units[a].floor(), units[b] - units[b].floor());
        rb.partial_cmp(&ra).unwrap().then(a.cmp(&b))
    });
    for &k in order.iter().take(short.max(0) as usize) {
        out[k] += 1;
    }
    std::array::from_fn(|k| out[k] as f64 / 100.)
}

fn compute(f: Density) -> [f64; POINTS] {
    let steps: Vec<f64> = (1..POINTS)
        .map(|k| simpson(f, (k - 1) as f64 / 20., k as f64 / 20., 64))
        .collect();
    let total: f64 = steps.iter().sum();
    let shares = std::array::from_fn(|k| {
        if k == 0 {
            0.
        } else {
            steps[k - 1] * 100. / total
        }
    });
    round_to_hundredths(&shares)
}

/// The preset curves, in the order they are offered.
pub fn presets() -> &'static [Preset] {
    static PRESETS: OnceLock<Vec<Preset>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        DEFINITIONS
            .iter()
            .map(|(name, shape, formula, f)| Preset {
                name,
                shape,
                formula,
                points: compute(*f),
            })
            .collect()
    })
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The preset with this name (case and outer spaces ignored).
pub fn preset(name: &str) -> Option<&'static Preset> {
    presets().iter().find(|p| same_name(p.name, name))
}

/// Where a listed curve comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The file's own curve.
    File,
    /// A curve the file marks as a default curve (`default_flag` Y).
    Default,
    /// An OpenXER preset, in the file already or not yet.
    Preset,
}

/// One curve as a user sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// The file's `curv_id`; `None` for a preset the file does not hold yet.
    pub id: Option<i64>,
    pub name: String,
    pub points: [f64; POINTS],
    pub kind: Kind,
}

impl Curve {
    /// Presets and default curves cannot be renamed, changed or deleted.
    pub fn locked(&self) -> bool {
        self.kind != Kind::File
    }
}

fn row_of(doc: &Document, id: i64) -> Option<usize> {
    let t = doc.table(TABLE)?;
    t.rows.iter().position(|r| num(t.value(r, "curv_id")) == id)
}

fn read_points(doc: &Document, row: usize) -> [f64; POINTS] {
    std::array::from_fn(|k| float(&records::get(doc, TABLE, row, &format!("pct_usage_{k}"))))
}

fn same_points(a: &[f64; POINTS], b: &[f64; POINTS]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

fn file_curve(doc: &Document, row: usize) -> Curve {
    let name = records::get(doc, TABLE, row, "curv_name");
    let points = read_points(doc, row);
    let kind = if records::get(doc, TABLE, row, "default_flag") == "Y" {
        Kind::Default
    } else if preset(&name).is_some_and(|p| same_points(&p.points, &points)) {
        Kind::Preset
    } else {
        Kind::File
    };
    Curve {
        id: Some(num(&records::get(doc, TABLE, row, "curv_id"))),
        name,
        points,
        kind,
    }
}

/// The file's curves in file order, then each preset whose name the file
/// does not hold.
pub fn list(doc: &Document) -> Vec<Curve> {
    let rows = doc.table(TABLE).map_or(0, |t| t.rows.len());
    let mut out: Vec<Curve> = (0..rows)
        .map(|r| file_curve(doc, r))
        .filter(|c| c.id.is_some_and(|id| id != 0))
        .collect();
    for p in presets() {
        if !out.iter().any(|c| same_name(&c.name, p.name)) {
            out.push(Curve {
                id: None,
                name: p.name.into(),
                points: p.points,
                kind: Kind::Preset,
            });
        }
    }
    out
}

/// The file's curve with this ID.
pub fn by_id(doc: &Document, id: i64) -> Option<Curve> {
    row_of(doc, id).map(|r| file_curve(doc, r))
}

/// The curve with this name: the file's first, else a preset.
pub fn find(doc: &Document, name: &str) -> Option<Curve> {
    list(doc).into_iter().find(|c| same_name(&c.name, name))
}

/// The `curv_id` for a curve name, copying a preset into the file when the
/// file does not hold it yet.
pub fn ensure(doc: &mut Document, name: &str) -> Result<i64, String> {
    let curve = find(doc, name).ok_or_else(|| {
        let names: Vec<String> = list(doc).into_iter().map(|c| c.name).collect();
        format!(
            "No resource curve named {name:?}. Curves: {}.",
            names.join(", ")
        )
    })?;
    if let Some(id) = curve.id {
        return Ok(id);
    }
    let id = records::next_id(doc, TABLE, "curv_id");
    let names: Vec<String> = (0..POINTS).map(|k| format!("pct_usage_{k}")).collect();
    let mut values = vec![
        ("curv_id", id.to_string()),
        ("curv_name", curve.name.clone()),
        ("default_flag", "N".to_string()),
    ];
    for (k, column) in names.iter().enumerate() {
        values.push((column.as_str(), records::format_number(curve.points[k])));
    }
    records::add_row(doc, TABLE, &values);
    Ok(id)
}

/// Assignments that use the curve.
pub fn uses(doc: &Document, id: i64) -> usize {
    doc.table("TASKRSRC").map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "curv_id")) == id)
            .count()
    })
}

/// The curve an assignment (TASKRSRC row) uses, if any.
pub fn of_assignment(doc: &Document, row: usize) -> Option<i64> {
    let id = num(&records::get(doc, "TASKRSRC", row, "curv_id"));
    (id != 0).then_some(id)
}

/// The file's own curve named "Manual", which an assignment with manual
/// period values points at when the file holds one (an export from the
/// reference stored such a row's `curv_id` as 9; the row it names was not
/// seen).
pub fn manual_curve(doc: &Document) -> Option<i64> {
    let t = doc.table(TABLE)?;
    t.rows
        .iter()
        .find(|r| same_name(t.value(r, "curv_name"), crate::manual_units::MANUAL))
        .map(|r| num(t.value(r, "curv_id")))
        .filter(|id| *id != 0)
}

/// What an assignment's Curve column shows: "Manual" once it has manual
/// period values, else its curve's name (its `curv_id` when the file lacks
/// that curve); `None` with neither.
pub fn curve_label(doc: &Document, row: usize) -> Option<String> {
    if crate::manual_units::is_manual(doc, row) {
        return Some(crate::manual_units::MANUAL.into());
    }
    of_assignment(doc, row).map(|id| by_id(doc, id).map_or(id.to_string(), |c| c.name))
}

/// Remove an assignment's manual period values (Clear in the curve
/// picker): the run columns are emptied and the curve with them, so the
/// units spread evenly (or by a curve given next) again. The totals stay.
/// False when it had none.
pub fn clear_manual(doc: &mut Document, row: usize) -> bool {
    if !crate::manual_units::is_manual(doc, row) {
        return false;
    }
    for column in crate::manual_units::COLUMNS {
        records::set(doc, "TASKRSRC", row, column, String::new());
    }
    records::set(doc, "TASKRSRC", row, "curv_id", String::new());
    true
}

/// Give an assignment (TASKRSRC row) a curve by name, or none with `None`
/// or a blank name. A number that is not a curve name but is a `curv_id` of
/// the file is taken as that ID. False when the assignment already had it.
/// An assignment with manual period values loses them (the caller asks
/// first).
pub fn set_assignment_curve(
    doc: &mut Document,
    row: usize,
    name: Option<&str>,
) -> Result<bool, String> {
    if doc.table("TASKRSRC").is_none_or(|t| row >= t.rows.len()) {
        return Err("No such assignment.".into());
    }
    let wanted = match name.map(str::trim).filter(|n| !n.is_empty()) {
        None => None,
        Some(n) => match find(doc, n) {
            Some(_) => Some(ensure(doc, n)?),
            None => match n
                .parse::<i64>()
                .ok()
                .filter(|id| row_of(doc, *id).is_some())
            {
                Some(id) => Some(id),
                None => return Err(ensure(doc, n).unwrap_err()),
            },
        },
    };
    let manual = clear_manual(doc, row);
    if !manual && of_assignment(doc, row) == wanted {
        return Ok(false);
    }
    let text = wanted.map_or(String::new(), |id| id.to_string());
    records::set(doc, "TASKRSRC", row, "curv_id", text);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Closed forms of each preset's cumulative share F(t), worked out by
    /// hand, to check the numeric integration.
    fn erf(x: f64) -> f64 {
        // Abramowitz and Stegun 7.1.26, absolute error below 1.5e-7.
        let s = x.signum();
        let x = x.abs();
        let t = 1. / (1. + 0.3275911 * x);
        let y = 1.
            - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
                + 0.254829592)
                * t
                * (-x * x).exp();
        s * y
    }
    fn phi(t: f64, mean: f64, sd: f64) -> f64 {
        0.5 * (1. + erf((t - mean) / (sd * 2_f64.sqrt())))
    }
    fn truncated(t: f64, mean: f64, sd: f64) -> f64 {
        (phi(t, mean, sd) - phi(0., mean, sd)) / (phi(1., mean, sd) - phi(0., mean, sd))
    }
    fn trapezoid_cdf(t: f64) -> f64 {
        // Area 0.8: 0.1 up, 0.6 level, 0.1 down.
        let a = if t <= 0.2 {
            t * t / 0.4
        } else if t <= 0.8 {
            0.1 + (t - 0.2)
        } else {
            0.8 - (1. - t) * (1. - t) / 0.4
        };
        a / 0.8
    }
    fn closed_form(name: &str, t: f64) -> f64 {
        match name {
            "Even" => t,
            "Early Weighted" => 1. - (1. - t).powi(3),
            "Late Weighted" => t.powi(3),
            "Ramp Up" => t * t,
            "Ramp Down" => 1. - (1. - t) * (1. - t),
            "Bell, Centered" => truncated(t, 0.5, 0.15),
            "Bell, Early Peak" => truncated(t, 0.3, 0.12),
            "Bell, Late Peak" => truncated(t, 0.7, 0.12),
            "Two Peaks" => {
                let a = phi(t, 0.25, 0.1) + phi(t, 0.75, 0.1);
                let lo = phi(0., 0.25, 0.1) + phi(0., 0.75, 0.1);
                let hi = phi(1., 0.25, 0.1) + phi(1., 0.75, 0.1);
                (a - lo) / (hi - lo)
            }
            "Steady with Ramps" => trapezoid_cdf(t),
            other => panic!("no closed form for {other}"),
        }
    }

    #[test]
    fn presets_total_100_and_match_their_formulas() {
        assert!((8..=12).contains(&presets().len()));
        for p in presets() {
            let total: f64 = p.points.iter().sum();
            assert!((total - 100.).abs() < 1e-9, "{} totals {total}", p.name);
            assert_eq!(p.points[0], 0., "{} starts at 0", p.name);
            for (k, v) in p.points.iter().enumerate() {
                assert!(
                    ((v * 100.).round() - v * 100.).abs() < 1e-6,
                    "{} point {k} is not a hundredth",
                    p.name
                );
                if k == 0 {
                    continue;
                }
                let exact = 100.
                    * (closed_form(p.name, k as f64 / 20.)
                        - closed_form(p.name, (k - 1) as f64 / 20.));
                // Rounding to hundredths moves a point by less than 0.01.
                assert!(
                    (v - exact).abs() < 0.01 + 1e-6,
                    "{} point {k}: {v} vs {exact}",
                    p.name
                );
            }
        }
        let even = preset("even").unwrap();
        assert!(even.points[1..].iter().all(|v| *v == 5.));
        assert!(presets()
            .iter()
            .all(|p| !p.formula.is_empty() && !p.shape.is_empty()));
    }

    #[test]
    fn rounding_keeps_the_total() {
        let shares: [f64; POINTS] = std::array::from_fn(|k| {
            if k == 0 {
                0.
            } else {
                100. / 20. + (k as f64 - 10.5) / 30.
            }
        });
        let sum: f64 = shares.iter().sum();
        let shares = shares.map(|s| s * 100. / sum);
        let r = round_to_hundredths(&shares);
        assert!((r.iter().sum::<f64>() - 100.).abs() < 1e-9);
    }
}

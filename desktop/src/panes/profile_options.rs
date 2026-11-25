//! Profile Options of the Activity and Resource Usage Profiles: per series
//! a By Date switch (its bars) and a Cumulative switch (its running-total
//! curve) with its own color; units or costs; on the activity profile the
//! resource types counted (and expenses, a cost); the limit, overallocation,
//! overtime and availability of the resource profile; and the Graph tab:
//! sight lines, the legend, the background, 3D bars, values on the bars and
//! the average (each period's total divided by a number, or by the work
//! hours of the period). Everything is saved with the layout. The dialog
//! applies on OK or Apply; these are view settings, never undo steps.
use super::usage::{self, ProfileOptions};
use super::*;
use crate::dialog_kit::{self as kit, Entry, VTab};
use crate::gantt::settings::Rgb;

/// One series of a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Series {
    Budgeted,
    Actual,
    RemainingEarly,
    RemainingLate,
    /// The activity profile only, when a stored baseline fills the slot.
    ProjectBaseline,
    PrimaryBaseline,
}

impl Series {
    pub const ALL: [Series; 6] = [
        Series::Budgeted,
        Series::Actual,
        Series::RemainingEarly,
        Series::RemainingLate,
        Series::ProjectBaseline,
        Series::PrimaryBaseline,
    ];
    /// The resource profile's series.
    pub const RESOURCE: [Series; 4] = [
        Series::Budgeted,
        Series::Actual,
        Series::RemainingEarly,
        Series::RemainingLate,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn key(self) -> &'static str {
        match self {
            Series::Budgeted => "budgeted",
            Series::Actual => "actual",
            Series::RemainingEarly => "remaining_early",
            Series::RemainingLate => "remaining_late",
            Series::ProjectBaseline => "project_baseline",
            Series::PrimaryBaseline => "primary_baseline",
        }
    }
    pub fn caption(self) -> &'static str {
        match self {
            Series::Budgeted => "Budgeted",
            Series::Actual => "Actual",
            Series::RemainingEarly => "Remaining Early",
            Series::RemainingLate => "Remaining Late",
            Series::ProjectBaseline => "Project Baseline",
            Series::PrimaryBaseline => "Primary Baseline",
        }
    }
    /// OpenXER's own colors (fill, edge), the ones the profiles drew before
    /// the colors could be chosen.
    pub fn default_colors(self) -> (Color32, Color32) {
        match self {
            Series::Budgeted => usage::BUDGET,
            Series::Actual => usage::ACTUAL,
            Series::RemainingEarly => usage::REMAINING,
            Series::RemainingLate => usage::REMAINING_LATE,
            Series::ProjectBaseline => usage::BASELINE_SERIES,
            Series::PrimaryBaseline => usage::PRIMARY_SERIES,
        }
    }
    /// Its running-total curve is dashed: the late curve against the early
    /// one (the two make the band between early and late).
    pub fn dashed(self) -> bool {
        self == Series::RemainingLate
    }
}

/// The fill and edge of a series drawn in `fill`: a default fill keeps its
/// own edge, any other fill a darker shade of itself. The curve takes the
/// edge.
pub fn colors(fill: Rgb) -> (Color32, Color32) {
    let c = fill.color();
    if let Some(s) = Series::ALL.iter().find(|s| s.default_colors().0 == c) {
        return s.default_colors();
    }
    let dark = |v: u8| (v as f32 * 0.6) as u8;
    (c, Color32::from_rgb(dark(c.r()), dark(c.g()), dark(c.b())))
}

/// One series' switches and color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Show {
    pub by_date: bool,
    pub cumulative: bool,
    pub color: Rgb,
}

/// Each profile's series, in `Series::ALL` order (the resource profile
/// uses the first four).
pub type SeriesSet = [Show; 6];

/// Out-of-the-box series of a profile (`defaults::profile_series`).
pub fn default_series(resource: bool) -> SeriesSet {
    let flags = crate::defaults::profile_series(resource);
    Series::ALL.map(|s| {
        let (by_date, cumulative) = flags.get(s.index()).copied().unwrap_or((false, false));
        Show {
            by_date,
            cumulative,
            color: Rgb::from_color(s.default_colors().0),
        }
    })
}

/// The series a layout saved before the switches existed draws: every
/// series of the activity profile as bars, actual and remaining early on
/// the resource profile, no curves.
pub fn earlier_series(resource: bool) -> SeriesSet {
    let flags = crate::defaults::earlier_profile_series(resource);
    let mut set = default_series(resource);
    for (show, (by_date, cumulative)) in set.iter_mut().zip(flags) {
        show.by_date = by_date;
        show.cumulative = cumulative;
    }
    set
}

/// The activity profile's resource types: Labor, Nonlabor, Material and
/// Expenses (a cost, counted with Costs only).
pub const TYPES: [(&str, &str); 4] = [
    ("labor", "Labor"),
    ("nonlabor", "Nonlabor"),
    ("material", "Material"),
    ("expenses", "Expenses"),
];

/// Horizontal lines across the chart at the left scale's steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HLines {
    #[default]
    Dotted,
    Solid,
    None,
}

impl HLines {
    pub const ALL: [(HLines, &'static str, &'static str); 3] = [
        (HLines::Dotted, "dotted", "Dotted"),
        (HLines::Solid, "solid", "Solid"),
        (HLines::None, "none", "None"),
    ];
    fn key(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|x| x.0 == self)
            .map_or("dotted", |x| x.1)
    }
    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().find(|x| x.1 == key).map(|x| x.0)
    }
}

/// The Graph tab.
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    /// Vertical lines at the timescale's upper (major) and lower (minor)
    /// steps.
    pub major: bool,
    pub minor: bool,
    pub hlines: HLines,
    pub line_color: Rgb,
    pub background: Rgb,
    pub bars_3d: bool,
    /// Each bar's value written over it.
    pub values: bool,
    /// Calculate Average: each period's total (the bars and the limit, on
    /// the left scale) divided by `divisor`, or with `hours_per_period` by
    /// the work hours of the period (Admin Preferences > Time Periods); the
    /// left scale is labelled `unit`. The running totals keep their sums.
    pub average: bool,
    pub divisor: f64,
    pub hours_per_period: bool,
    pub unit: String,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            major: true,
            minor: false,
            hlines: HLines::Dotted,
            line_color: Rgb::from_color(usage::LINE),
            background: Rgb::WHITE,
            bars_3d: false,
            values: false,
            average: false,
            divisor: 1.,
            hours_per_period: false,
            unit: String::new(),
        }
    }
}

/// Longest unit label kept.
const MAX_UNIT: usize = 40;

impl Graph {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "major": self.major,
            "minor": self.minor,
            "hlines": self.hlines.key(),
            "line_color": self.line_color.to_string(),
            "background": self.background.to_string(),
            "bars_3d": self.bars_3d,
            "values": self.values,
            "average": self.average,
            "divisor": self.divisor,
            "hours_per_period": self.hours_per_period,
            "unit": self.unit,
        })
    }
    pub fn restore(&mut self, v: &serde_json::Value) {
        for (key, flag) in [
            ("major", &mut self.major),
            ("minor", &mut self.minor),
            ("bars_3d", &mut self.bars_3d),
            ("values", &mut self.values),
            ("average", &mut self.average),
            ("hours_per_period", &mut self.hours_per_period),
        ] {
            if let Some(b) = v[key].as_bool() {
                *flag = b;
            }
        }
        if let Some(h) = v["hlines"].as_str().and_then(HLines::from_key) {
            self.hlines = h;
        }
        for (key, color) in [
            ("line_color", &mut self.line_color),
            ("background", &mut self.background),
        ] {
            if let Some(c) = rgb(&v[key]) {
                *color = c;
            }
        }
        if let Some(d) = v["divisor"].as_f64().filter(|d| d.is_finite() && *d > 0.) {
            self.divisor = d;
        }
        if let Some(u) = v["unit"].as_str() {
            self.unit = u.chars().take(MAX_UNIT).collect();
        }
    }
    /// What each period's total is divided by: the divisor, or the period's
    /// work hours. `None` when there is no average to take.
    pub fn divisors(&self, periods: &[(i64, i64)]) -> Option<Vec<f64>> {
        if !self.average {
            return None;
        }
        let p = crate::format::Periods::DEFAULT;
        Some(
            periods
                .iter()
                .map(|&(a, b)| {
                    if !self.hours_per_period {
                        return self.divisor.max(f64::MIN_POSITIVE);
                    }
                    let days = (b - a) as f64 / (24. * 60.);
                    let minutes = if days <= 1.5 {
                        p.day
                    } else if days <= 7.5 {
                        p.week
                    } else if days <= 31.5 {
                        p.month
                    } else if days <= 92.5 {
                        3 * p.month
                    } else {
                        p.year
                    };
                    minutes as f64 / 60.
                })
                .collect(),
        )
    }
}

fn rgb(v: &serde_json::Value) -> Option<Rgb> {
    Rgb::try_from(v.as_str()?.to_string()).ok()
}

pub fn series_json(set: &SeriesSet) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for s in Series::ALL {
        let x = set[s.index()];
        map.insert(
            s.key().into(),
            serde_json::json!({
                "by_date": x.by_date,
                "cumulative": x.cumulative,
                "color": x.color.to_string(),
            }),
        );
    }
    serde_json::Value::Object(map)
}

pub fn restore_series(set: &mut SeriesSet, v: &serde_json::Value) {
    for s in Series::ALL {
        let x = &v[s.key()];
        let show = &mut set[s.index()];
        if let Some(b) = x["by_date"].as_bool() {
            show.by_date = b;
        }
        if let Some(b) = x["cumulative"].as_bool() {
            show.cumulative = b;
        }
        if let Some(c) = rgb(&x["color"]) {
            show.color = c;
        }
    }
}

/// Profile Options while open: a working copy of the options and of what
/// the dialog sets beside them.
#[derive(Clone, Debug)]
pub struct Draft {
    pub options: ProfileOptions,
    /// Show Actual and Remaining of this profile.
    pub both: bool,
    pub costs: bool,
    /// The Resource Usage Profile's dialog (else the activity profile's).
    pub resource: bool,
    /// 0: Data, 1: Graph.
    pub tab: usize,
}

const W: f32 = 580.;
const H: f32 = 446.;

impl App {
    /// Opens Profile Options for the profile on show.
    pub(crate) fn open_profile_options(&mut self) {
        let u = &self.panes.usage;
        let resource = self.panes.bottom == BottomView::ResourceUsageProfile;
        let both = if resource {
            u.stacked_resource
        } else {
            u.stacked_activity
        };
        self.panes.profile_dialog = Some(Draft {
            options: u.profile.clone(),
            both,
            costs: u.costs,
            resource,
            tab: 0,
        });
    }

    /// The baselines the activity profile can draw: the project and the
    /// primary baseline, each when a stored baseline is assigned to it.
    pub(crate) fn profile_baselines(&self) -> [bool; 2] {
        let compare = self.baseline_compare();
        [
            crate::baselines::Slot::Project,
            crate::baselines::Slot::Primary,
        ]
        .map(|slot| {
            matches!(
                compare.source(slot),
                crate::baselines::data::Source::Baseline(b) if b.project.is_some()
            )
        })
    }

    fn apply_profile_draft(&mut self, d: &Draft) {
        let u = &mut self.panes.usage;
        u.profile = d.options.clone();
        u.costs = d.costs;
        if d.resource {
            u.stacked_resource = d.both;
        } else {
            u.stacked_activity = d.both;
        }
    }

    pub(crate) fn profile_options_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.panes.profile_dialog.take() else {
            return;
        };
        let baselines = self.profile_baselines();
        let title = if d.resource {
            "Resource Profile Options"
        } else {
            "Activity Profile Options"
        };
        let (clicked, closed, actions) =
            kit::window(ctx, "usage_profile_options", title, Vec2::new(W, H), |c| {
                let tabs = [("Data", "data"), ("Graph", "graph")].map(|(label, key)| VTab {
                    label,
                    key,
                    unavailable: None,
                });
                if let Some(k) = kit::tabs_spread(
                    c,
                    "usage_profile_tab_",
                    (5., 26., 460., H - 32.),
                    &tabs,
                    d.tab,
                ) {
                    d.tab = k;
                }
                if d.tab == 0 {
                    data_tab(c, &mut d, baselines);
                } else {
                    graph_tab(c, &mut d.options.graph, &mut d.options.legend);
                }
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("usage_profile_options_ok"),
                        Entry::cancel("usage_profile_options_cancel"),
                        Entry::apply("usage_profile_options_apply", true),
                        Entry::Button(
                            "usage_profile_options_preferences",
                            "Preferences...",
                            true,
                            false,
                        ),
                        Entry::help("usage_profile_options_help"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        match clicked.as_deref() {
            Some("usage_profile_options_ok") => {
                self.apply_profile_draft(&d);
                return;
            }
            Some("usage_profile_options_cancel") => return,
            Some("usage_profile_options_apply") => self.apply_profile_draft(&d),
            Some("usage_profile_options_preferences") => self.open_user_preferences(),
            Some("usage_profile_options_help") => self.open_help_for("usage_profile_options"),
            _ => {}
        }
        if !closed {
            self.panes.profile_dialog = Some(d);
        }
    }
}

/// The Data tab: Display, Show Bars/Curves (with the remaining colors) and
/// the other data options.
fn data_tab(c: &mut kit::Canvas, d: &mut Draft, baselines: [bool; 2]) {
    let resource = d.resource;
    kit::section(c, 12., 34., 446., 70., "Display");
    if c.radio(
        "usage_profile_units",
        22.,
        58.,
        90.,
        18.,
        "Units",
        !d.costs,
        true,
    ) {
        d.costs = false;
    }
    if c.radio(
        "usage_profile_costs",
        22.,
        80.,
        90.,
        18.,
        "Cost",
        d.costs,
        true,
    ) {
        d.costs = true;
    }
    if !resource {
        for (k, (key, caption)) in TYPES.into_iter().enumerate() {
            let x = 170. + 120. * (k % 2) as f32;
            let y = 58. + 22. * (k / 2) as f32;
            // Expenses are a cost: they have no units.
            let enabled = key != "expenses" || d.costs;
            c.checkbox(
                &format!("usage_profile_type_{key}"),
                x,
                y,
                110.,
                18.,
                caption,
                &mut d.options.types[k],
                enabled,
            );
        }
    }
    let list: &[Series] = if resource {
        &Series::RESOURCE
    } else {
        &Series::ALL
    };
    let rows_top = 154.;
    let section_h = 50. + 21. * list.len() as f32 + 66.;
    kit::section(c, 12., 108., 446., section_h, "Show Bars/Curves");
    c.label(150., 132., 16., "By Date", true);
    c.label(220., 132., 16., "Cumulative", true);
    c.label(310., 132., 16., "Color", true);
    let set = if resource {
        &mut d.options.resource
    } else {
        &mut d.options.activity
    };
    for (k, s) in list.iter().enumerate() {
        let y = rows_top + 21. * k as f32;
        let baseline = match s {
            Series::ProjectBaseline => Some(baselines[0]),
            Series::PrimaryBaseline => Some(baselines[1]),
            _ => None,
        };
        let enabled = baseline.unwrap_or(true);
        c.label(22., y, 18., s.caption(), enabled);
        let show = &mut set[s.index()];
        c.checkbox(
            &format!("usage_profile_series_{}_by_date", s.key()),
            162.,
            y,
            16.,
            18.,
            "",
            &mut show.by_date,
            enabled,
        );
        c.checkbox(
            &format!("usage_profile_series_{}_cumulative", s.key()),
            238.,
            y,
            16.,
            18.,
            "",
            &mut show.cumulative,
            enabled,
        );
        // Remaining early and late take their colors below.
        if !matches!(s, Series::RemainingEarly | Series::RemainingLate) {
            kit::swatch(
                c,
                &format!("usage_profile_series_{}_color", s.key()),
                (300., y + 1., 64., 17.),
                &mut show.color,
                enabled,
            );
        }
    }
    let sub = rows_top + 21. * list.len() as f32 + 4.;
    kit::section(c, 24., sub, 360., 58., "Remaining Bars");
    c.label(34., sub + 34., 18., "Total Remaining", true);
    for (s, x) in [
        (Series::RemainingEarly, 190.),
        (Series::RemainingLate, 270.),
    ] {
        c.label(
            x + 8.,
            sub + 22.,
            14.,
            if s == Series::RemainingEarly {
                "Early"
            } else {
                "Late"
            },
            true,
        );
        kit::swatch(
            c,
            &format!("usage_profile_series_{}_color", s.key()),
            (x, sub + 36., 48., 17.),
            &mut set[s.index()].color,
            true,
        );
    }
    let top = 108. + section_h + 4.;
    if resource {
        kit::section(c, 12., top, 446., 92., "Additional Data Options");
        let o = &mut d.options;
        let units = !d.costs;
        c.checkbox(
            "usage_profile_option_limits",
            22.,
            top + 26.,
            160.,
            18.,
            "Show Limit",
            &mut o.limits,
            units,
        );
        c.checkbox(
            "usage_profile_option_overallocation",
            22.,
            top + 47.,
            160.,
            18.,
            "Show Overallocation",
            &mut o.overallocation,
            units,
        );
        c.checkbox(
            "usage_profile_option_availability",
            22.,
            top + 68.,
            160.,
            18.,
            "Availability",
            &mut o.availability,
            units,
        );
        c.checkbox(
            "usage_profile_option_overtime",
            200.,
            top + 26.,
            160.,
            18.,
            "Show Overtime",
            &mut o.overtime,
            true,
        );
        c.checkbox(
            "usage_profile_option_actual_remaining",
            200.,
            top + 47.,
            220.,
            18.,
            "Show Actual and Remaining",
            &mut d.both,
            true,
        );
    } else {
        kit::section(c, 12., top, 446., 50., "Additional Data Options");
        c.checkbox(
            "usage_profile_option_actual_remaining",
            22.,
            top + 26.,
            220.,
            18.,
            "Show Actual and Remaining",
            &mut d.both,
            true,
        );
    }
}

/// The Graph tab: vertical and horizontal lines, then the legend, 3D bars,
/// values, background and the average.
fn graph_tab(c: &mut kit::Canvas, g: &mut Graph, legend: &mut bool) {
    kit::section(c, 12., 34., 446., 78., "Vertical Lines");
    c.checkbox(
        "usage_profile_graph_major",
        22.,
        58.,
        120.,
        18.,
        "Major",
        &mut g.major,
        true,
    );
    c.checkbox(
        "usage_profile_graph_minor",
        22.,
        80.,
        120.,
        18.,
        "Minor",
        &mut g.minor,
        true,
    );
    kit::section(c, 12., 116., 446., 98., "Horizontal Lines");
    for (k, (choice, key, caption)) in HLines::ALL.into_iter().enumerate() {
        let y = 140. + 22. * k as f32;
        if c.radio(
            &format!("usage_profile_graph_lines_{key}"),
            22.,
            y,
            100.,
            18.,
            caption,
            g.hlines == choice,
            true,
        ) {
            g.hlines = choice;
        }
    }
    c.label(190., 138., 16., "Line Color", g.hlines != HLines::None);
    kit::swatch(
        c,
        "usage_profile_graph_line_color",
        (190., 156., 64., 20.),
        &mut g.line_color,
        g.hlines != HLines::None,
    );
    kit::section(c, 12., 218., 446., 210., "Additional Display Options");
    c.checkbox(
        "usage_profile_option_legend",
        22.,
        242.,
        140.,
        18.,
        "Show Legend",
        legend,
        true,
    );
    c.checkbox(
        "usage_profile_graph_3d",
        22.,
        263.,
        140.,
        18.,
        "3D Bars",
        &mut g.bars_3d,
        true,
    );
    c.checkbox(
        "usage_profile_graph_values",
        22.,
        284.,
        140.,
        18.,
        "Show Values",
        &mut g.values,
        true,
    );
    c.label(190., 240., 16., "Background Color", true);
    kit::swatch(
        c,
        "usage_profile_graph_background",
        (190., 258., 64., 20.),
        &mut g.background,
        true,
    );
    c.checkbox(
        "usage_profile_graph_average",
        22.,
        314.,
        160.,
        18.,
        "Calculate Average",
        &mut g.average,
        true,
    );
    let on = g.average;
    c.label(40., 336., 16., "Divide each period by:", on);
    kit::number_spin(
        c,
        "usage_profile_graph_divisor",
        (40., 354., 56., 20.),
        &mut g.divisor,
        1.,
        0.01..=1_000_000.,
        on && !g.hours_per_period,
    );
    c.checkbox(
        "usage_profile_graph_hours",
        110.,
        355.,
        240.,
        18.,
        "Use the work hours of each period",
        &mut g.hours_per_period,
        on,
    );
    c.label(40., 382., 16., "Unit of Measure:", on);
    c.edit(
        "usage_profile_graph_unit",
        40.,
        400.,
        104.,
        20.,
        &mut g.unit,
        on,
        egui::Align::Min,
    );
    if g.unit.chars().count() > MAX_UNIT {
        g.unit = g.unit.chars().take(MAX_UNIT).collect();
    }
}

impl ProfileOptions {
    /// The series set of the activity profile or of the resource profile.
    pub fn series(&self, resource: bool) -> &SeriesSet {
        if resource {
            &self.resource
        } else {
            &self.activity
        }
    }
    pub fn series_mut(&mut self, resource: bool) -> &mut SeriesSet {
        if resource {
            &mut self.resource
        } else {
            &mut self.activity
        }
    }
    /// The profile draws any running-total curve.
    pub fn any_cumulative(&self, resource: bool) -> bool {
        self.series(resource).iter().any(|s| s.cumulative)
    }
}

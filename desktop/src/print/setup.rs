//! Page Setup settings: page, margins, header, footer and print options.
//! One setup is kept per activity layout in the preferences table (JSON).
//! Lengths are inches, as the dialog shows them.
use serde::{Deserialize, Serialize};

/// Preference key holding `{layout name: setup}`.
pub const PREF_KEY: &str = "print_setup_v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Portrait,
    #[default]
    Landscape,
    /// Picks Portrait or Landscape, whichever prints larger (ties go to
    /// fewer pages) for the content being printed.
    Automatic,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Scaling {
    /// Adjust to a percent of normal size.
    Adjust { percent: u32 },
    /// Fit to pages wide by pages tall; `tall == 0` leaves the height free.
    Fit { wide: u32, tall: u32 },
    /// Keep text at normal size and stretch the timescale over pages wide.
    FitTimescale { wide: u32 },
}

/// A paper size: name and portrait width and height in inches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paper {
    pub name: &'static str,
    pub width: f32,
    pub height: f32,
}

const fn mm(value: f32) -> f32 {
    value / 25.4
}

/// Paper sizes offered in the Paper size list.
pub const PAPERS: [Paper; 11] = [
    Paper {
        name: "Letter",
        width: 8.5,
        height: 11.,
    },
    Paper {
        name: "Legal",
        width: 8.5,
        height: 14.,
    },
    Paper {
        name: "Tabloid",
        width: 11.,
        height: 17.,
    },
    Paper {
        name: "Executive",
        width: 7.25,
        height: 10.5,
    },
    Paper {
        name: "A5",
        width: mm(148.),
        height: mm(210.),
    },
    Paper {
        name: "A4",
        width: mm(210.),
        height: mm(297.),
    },
    Paper {
        name: "A3",
        width: mm(297.),
        height: mm(420.),
    },
    Paper {
        name: "B4 (JIS)",
        width: mm(257.),
        height: mm(364.),
    },
    Paper {
        name: "ANSI C",
        width: 17.,
        height: 22.,
    },
    Paper {
        name: "ANSI D",
        width: 22.,
        height: 34.,
    },
    Paper {
        name: "ANSI E",
        width: 34.,
        height: 44.,
    },
];

pub fn paper(name: &str) -> Paper {
    PAPERS
        .iter()
        .copied()
        .find(|p| p.name == name)
        .unwrap_or(PAPERS[0])
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Margins {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Which pages carry a header or footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncludeOn {
    FirstPage,
    LastPage,
    AllPages,
    NoPages,
}
impl IncludeOn {
    /// Does a page row that is the `first` and/or the `last` of the job
    /// carry the band?
    pub fn on_block(self, first: bool, last: bool) -> bool {
        match self {
            Self::FirstPage => first,
            Self::LastPage => last,
            Self::AllPages => true,
            Self::NoPages => false,
        }
    }
    pub const ALL: [IncludeOn; 4] = [
        Self::FirstPage,
        Self::LastPage,
        Self::AllPages,
        Self::NoPages,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::FirstPage => "First Page",
            Self::LastPage => "Last Page",
            Self::AllPages => "All Pages",
            Self::NoPages => "No Pages",
        }
    }
    /// Is the band printed on page `index` (from 0) of `count`?
    pub fn on(self, index: usize, count: usize) -> bool {
        match self {
            Self::FirstPage => index == 0,
            Self::LastPage => index + 1 == count,
            Self::AllPages => true,
            Self::NoPages => false,
        }
    }
}

/// What one header or footer section shows. Pictures and logos are not
/// offered: sections hold text, a legend or a revision box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Content {
    None,
    Text,
    Legend,
    Revisions,
}
impl Content {
    pub const ALL: [Content; 4] = [Self::None, Self::Text, Self::Legend, Self::Revisions];
    /// Caption in the section grid; the legend caption names the chart.
    pub fn label(self, network: bool) -> &'static str {
        match self {
            Self::None => "(None)",
            Self::Text => "Text",
            Self::Legend if network => "Activity Network Legend",
            Self::Legend => "Gantt Chart Legend",
            Self::Revisions => "Revision Box",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub date: String,
    pub revision: String,
    pub checked: String,
    pub approved: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub content: Content,
    /// Text with `[field]` tokens (see `FIELDS`).
    pub text: String,
    pub align: TextAlign,
    pub revision_title: String,
    pub revisions: Vec<Revision>,
}

impl Section {
    pub fn text(text: &str, align: TextAlign) -> Self {
        Self {
            content: Content::Text,
            text: text.into(),
            align,
            revision_title: "Revisions".into(),
            revisions: vec![],
        }
    }
    pub fn with(content: Content) -> Self {
        Self {
            content,
            ..Self::text("", TextAlign::Left)
        }
    }
}

/// Header or footer band.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub include_on: IncludeOn,
    /// Band height in inches.
    pub height: f32,
    pub dividers: bool,
    /// Section widths in percent of the band, summing to 100.
    pub widths: Vec<f32>,
    pub sections: Vec<Section>,
}

pub const MAX_SECTIONS: usize = 5;

impl Band {
    pub fn count(&self) -> usize {
        self.sections.len()
    }
    /// Divides the band into `n` equal sections, keeping existing sections.
    pub fn divide(&mut self, n: usize) {
        let n = n.clamp(1, MAX_SECTIONS);
        self.sections
            .resize(n, Section::text("", TextAlign::Center));
        self.widths = equal_widths(n);
    }
    /// Section edges as fractions of the band width (n + 1 values).
    pub fn edges(&self) -> Vec<f32> {
        let total: f32 = self.widths.iter().sum::<f32>().max(1e-3);
        let mut edges = vec![0.];
        for w in &self.widths {
            edges.push(edges.last().unwrap() + w / total);
        }
        if let Some(last) = edges.last_mut() {
            *last = 1.;
        }
        edges
    }
    /// Moves divider `k` (between sections k and k + 1) to `fraction`,
    /// keeping every section at least 5% wide.
    pub fn move_divider(&mut self, k: usize, fraction: f32) {
        let edges = self.edges();
        if k + 2 >= edges.len() {
            return;
        }
        let lo = edges[k] + 0.05;
        let hi = edges[k + 2] - 0.05;
        if lo > hi {
            return;
        }
        let at = fraction.clamp(lo, hi);
        let mut new_edges = edges;
        new_edges[k + 1] = at;
        self.widths = new_edges
            .windows(2)
            .map(|w| ((w[1] - w[0]) * 1000.).round() / 10.)
            .collect();
    }
}

fn equal_widths(n: usize) -> Vec<f32> {
    let each = (1000. / n as f32).round() / 10.;
    (0..n).map(|_| each).collect()
}

/// Options tab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintOptions {
    /// Timescale start and finish expressions (`DD-1W`, `PF+1W`) or dates.
    pub timescale_start: String,
    pub timescale_finish: String,
    pub activity_table: bool,
    pub all_columns: bool,
    pub grid_lines: bool,
    pub gantt_chart: bool,
    pub relationships: bool,
    pub break_every_group: bool,
    /// Print the bottom pane under the table and Gantt when it shows a
    /// usage profile, a usage spreadsheet or Trace Logic.
    #[serde(default = "yes")]
    pub profile: bool,
    #[serde(default = "yes")]
    pub spreadsheet: bool,
    #[serde(default = "yes")]
    pub trace_logic: bool,
    /// The Gantt prints the span the chart pane shows, at its zoom (a
    /// narrower span is extended to fill the page). Off: the expressions
    /// above. Setups saved before this existed keep their expressions.
    #[serde(default)]
    pub same_as_screen: bool,
    /// A page that starts inside a group repeats the group's band row at
    /// its top.
    #[serde(default)]
    pub repeat_band: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSetup {
    pub orientation: Orientation,
    pub scaling: Scaling,
    pub paper: String,
    pub margins: Margins,
    pub header: Band,
    pub footer: Band,
    pub options: PrintOptions,
    /// May a printout run to pages that sit side by side? Off: one page
    /// across whatever the scaling says. Setups saved before this existed
    /// keep tiling.
    #[serde(default = "yes")]
    pub allow_tiling: bool,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            // Landscape, as the reference starts; the legacy defaults of
            // tests pick the orientation from the content.
            orientation: if crate::defaults::reference() {
                Orientation::Landscape
            } else {
                Orientation::Automatic
            },
            scaling: Scaling::Fit { wide: 1, tall: 0 },
            paper: "Letter".into(),
            margins: Margins {
                top: 0.5,
                left: 0.5,
                right: 0.5,
                bottom: 0.5,
            },
            header: Band {
                include_on: IncludeOn::AllPages,
                height: 0.35,
                dividers: true,
                widths: equal_widths(3),
                sections: vec![
                    Section::text("[project_id]  [project_name]", TextAlign::Left),
                    Section::text("[layout_name]", TextAlign::Center),
                    Section::text("Data Date: [data_date]", TextAlign::Right),
                ],
            },
            footer: Band {
                include_on: IncludeOn::AllPages,
                height: 0.6,
                dividers: true,
                widths: equal_widths(3),
                sections: vec![
                    Section::with(Content::Legend),
                    Section::text("[filter_list]", TextAlign::Center),
                    Section::text(
                        "Printed [print_date]\nPage [page_number] of [total_pages]",
                        TextAlign::Right,
                    ),
                ],
            },
            options: PrintOptions {
                timescale_start: "DD-1W".into(),
                timescale_finish: "PF+1W".into(),
                activity_table: true,
                all_columns: true,
                grid_lines: true,
                gantt_chart: true,
                relationships: true,
                break_every_group: false,
                profile: true,
                spreadsheet: true,
                trace_logic: true,
                same_as_screen: true,
                repeat_band: false,
            },
            allow_tiling: false,
        }
    }
}

impl PageSetup {
    /// Page width and height in inches for the orientation (Automatic
    /// shows Landscape until the content picks one).
    pub fn page_inches(&self) -> (f32, f32) {
        let p = paper(&self.paper);
        match self.orientation {
            Orientation::Portrait => (p.width, p.height),
            Orientation::Landscape | Orientation::Automatic => (p.height, p.width),
        }
    }
    /// The scaling with every "pages wide" held at 1 unless pages may sit
    /// side by side.
    pub fn effective_scaling(&self) -> Scaling {
        if self.allow_tiling {
            return self.scaling;
        }
        match self.scaling {
            Scaling::Fit { tall, .. } => Scaling::Fit { wide: 1, tall },
            Scaling::FitTimescale { .. } => Scaling::FitTimescale { wide: 1 },
            adjust => adjust,
        }
    }
    /// Clamps every number into its allowed range.
    pub fn sanitize(&mut self) {
        self.scaling = match self.scaling {
            Scaling::Adjust { percent } => Scaling::Adjust {
                percent: percent.clamp(10, 400),
            },
            Scaling::Fit { wide, tall } => Scaling::Fit {
                wide: wide.clamp(1, 50),
                tall: tall.min(200),
            },
            Scaling::FitTimescale { wide } => Scaling::FitTimescale {
                wide: wide.clamp(1, 50),
            },
        };
        let (w, h) = self.page_inches();
        let m = &mut self.margins;
        for v in [&mut m.top, &mut m.left, &mut m.right, &mut m.bottom] {
            *v = v.clamp(0., 4.);
        }
        m.left = m.left.min(w / 3.);
        m.right = m.right.min(w / 3.);
        m.top = m.top.min(h / 3.);
        m.bottom = m.bottom.min(h / 3.);
        for band in [&mut self.header, &mut self.footer] {
            band.height = band.height.clamp(0.1, (h / 4.).max(0.1));
            if band.sections.is_empty() {
                band.sections.push(Section::text("", TextAlign::Center));
            }
            band.sections.truncate(MAX_SECTIONS);
            if band.widths.len() != band.sections.len() || band.widths.iter().any(|w| *w <= 0.) {
                band.widths = equal_widths(band.sections.len());
            }
        }
    }
}

/// Text fields a section can show, as `[token]` and the menu caption.
pub const FIELDS: [(&str, &str); 12] = [
    ("project_id", "Project ID"),
    ("project_name", "Project Name"),
    ("layout_name", "Layout Name"),
    ("filter_list", "Filter List"),
    ("data_date", "Data Date"),
    ("start_date", "Project Start"),
    ("finish_date", "Project Finish"),
    ("print_date", "Print Date"),
    ("print_time", "Print Time"),
    ("page_number", "Page Number"),
    ("total_pages", "Total Pages"),
    ("scale", "Print Scale"),
];

/// Replaces `[token]` fields with `value(token)`; unknown tokens stay.
pub fn expand(text: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(']') {
            Some(close) => {
                let token = &after[..close];
                match value(token.trim()) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('[');
                        out.push_str(token);
                        out.push(']');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Anchor of a timescale expression: the same anchors the filter editor's
/// relative dates count from.
pub use crate::grid::relative::Anchor;

/// A parsed timescale expression: an anchor and an offset, or a fixed date
/// (model minutes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateExpr {
    Anchored {
        anchor: Anchor,
        offset: i64,
        unit: char,
    },
    Fixed(i64),
}

/// Longest offset accepted in an expression: 50 years of days.
pub const MAX_OFFSET_DAYS: i64 = 50 * 366;

/// Parses `DD-1W`, `PF+2M`, `PS`, `CD+3D`, `SW`, `SM-1W` (units D, W, M, Y) or a date in
/// the user's format. Malformed text, including text read back from a
/// damaged preference, is an error, never a panic.
pub fn parse_expr(
    text: &str,
    parse_date: impl Fn(&str) -> Option<i64>,
) -> Result<DateExpr, String> {
    let t = text.trim().to_ascii_uppercase();
    for (code, anchor) in [
        ("PS", Anchor::ProjectStart),
        ("PF", Anchor::ProjectFinish),
        ("DD", Anchor::DataDate),
        ("CD", Anchor::CurrentDate),
        ("SW", Anchor::StartOfWeek),
        ("SM", Anchor::StartOfMonth),
    ] {
        let Some(rest) = t.strip_prefix(code) else {
            continue;
        };
        let rest: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
        if rest.is_empty() {
            return Ok(DateExpr::Anchored {
                anchor,
                offset: 0,
                unit: 'D',
            });
        }
        if !rest.is_ascii() {
            return Err(format!("Cannot read {text:?}: unexpected characters"));
        }
        let (sign, body) = if let Some(body) = rest.strip_prefix('+') {
            (1, body)
        } else if let Some(body) = rest.strip_prefix('-') {
            (-1, body)
        } else {
            return Err(format!(
                "Cannot read {text:?}: expected + or - after {code}"
            ));
        };
        let (digits, unit) = match body.chars().last() {
            Some(c) if c.is_ascii_alphabetic() => (&body[..body.len() - 1], c),
            _ => (body, 'D'),
        };
        if !matches!(unit, 'D' | 'W' | 'M' | 'Y') {
            return Err(format!("Cannot read {text:?}: units are D, W, M or Y"));
        }
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("Cannot read {text:?}: expected a number"));
        }
        let n: i64 = digits
            .parse()
            .map_err(|_| format!("Cannot read {text:?}: the offset is too large"))?;
        let per = match unit {
            'W' => 7,
            'M' => 30,
            'Y' => 365,
            _ => 1,
        };
        if n.checked_mul(per).is_none_or(|days| days > MAX_OFFSET_DAYS) {
            return Err(format!(
                "Cannot read {text:?}: offsets reach at most 50 years"
            ));
        }
        return Ok(DateExpr::Anchored {
            anchor,
            offset: sign * n,
            unit,
        });
    }
    parse_date(text.trim())
        .map(DateExpr::Fixed)
        .ok_or_else(|| format!("Cannot read {text:?} as a date or expression"))
}

impl DateExpr {
    /// The text that parses back to this expression (`DD-1W`, `PF`); a
    /// fixed date is written by `day`.
    pub fn text(&self, day: impl Fn(i64) -> String) -> String {
        match *self {
            DateExpr::Fixed(minutes) => day(minutes),
            DateExpr::Anchored {
                anchor,
                offset,
                unit,
            } => {
                if offset == 0 {
                    return anchor.code().into();
                }
                let sign = if offset < 0 { '-' } else { '+' };
                format!("{}{sign}{}{unit}", anchor.code(), offset.unsigned_abs())
            }
        }
    }

    /// Resolves to model minutes given the anchors (model minutes); an
    /// overflow is an error.
    pub fn resolve(self, anchor_value: impl Fn(Anchor) -> i64) -> Result<i64, String> {
        use openxer_engine::calendar::DAY;
        match self {
            DateExpr::Fixed(m) => Ok(m),
            DateExpr::Anchored {
                anchor,
                offset,
                unit,
            } => {
                let per = match unit {
                    'W' => 7,
                    'M' => 30,
                    'Y' => 365,
                    _ => 1,
                };
                offset
                    .checked_mul(per)
                    .and_then(|days| days.checked_mul(DAY))
                    .and_then(|minutes| anchor_value(anchor).checked_add(minutes))
                    .ok_or_else(|| "The timescale date is out of range.".to_string())
            }
        }
    }
}

/// Preference holding the Page Setup of each view other than Activities
/// (a map from the view's key to its setup).
pub const VIEWS_PREF_KEY: &str = "print_setup_views_v1";

/// The preference that holds the setup of one layout or view: `base` (the
/// older map of every setup) followed by the name.
pub fn slot_key(base: &str, name: &str) -> String {
    format!("{base}/{name}")
}

/// The saved setup of one layout or view: its own entry, else its entry in
/// the older map of every setup (`base`), written before setups were kept
/// one to a preference.
pub fn load_slot(
    preference: impl Fn(&str) -> Option<String>,
    base: &str,
    name: &str,
) -> Option<PageSetup> {
    if let Some(own) = preference(&slot_key(base, name))
        .and_then(|text| serde_json::from_str::<PageSetup>(&text).ok())
    {
        return Some(own);
    }
    load_all(preference(base).as_deref()).remove(name)
}

/// Stored setups for every layout (the older map).
pub fn load_all(text: Option<&str>) -> std::collections::BTreeMap<String, PageSetup> {
    text.and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default()
}

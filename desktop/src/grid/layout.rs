//! Named activity layouts: columns, filter selection, group and sort, band
//! styles, Gantt view settings and the bottom pane, with a strict JSON form
//! for the preferences table. Invalid stored data is rejected as a whole.
use super::columns::Align;
use super::filters::{Criterion, FilterSelection, Op, UserFilter, MAX_LEVEL};
use serde_json::{json, Value as Json};

pub const STANDARD_NAME: &str = "Standard Layout";
pub const MAX_BAND_LEVELS: usize = 17;

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnSetting {
    pub id: String,
    /// Pixel width including the cell's right grid line.
    pub width: f32,
    /// A title set in Edit Column; `None` shows the default title.
    pub title: Option<String>,
    /// An alignment set in Edit Column; `None` uses the column's default.
    pub align: Option<Align>,
}

impl ColumnSetting {
    pub fn new(id: &str, width: f32) -> Self {
        Self {
            id: id.into(),
            width,
            title: None,
            align: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SortKey {
    pub column: String,
    pub descending: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupLevel {
    /// A column id (`wbs` groups by the WBS hierarchy).
    pub column: String,
    /// For WBS: deepest band level shown (1 = top level); `None` is All.
    pub to_level: Option<usize>,
    pub indent: bool,
    /// Group Interval: for dates `day`, `week`, `month`, `quarter` or
    /// `year`; for numbers, durations, units and costs a step typed as the
    /// column shows its values (`5d`, `100`). Empty groups by exact value.
    pub interval: String,
}

/// Font and color of one band level, as Group and Sort's Font & Color sets
/// them. `row_height` is the band row's pitch including its grid line; the
/// table never draws a band shorter than its font needs
/// (`band_row_height`).
#[derive(Clone, Debug, PartialEq)]
pub struct BandStyle {
    pub background: [u8; 3],
    pub text: [u8; 3],
    pub font_size: f32,
    pub bold: bool,
    pub row_height: f32,
}

impl Default for BandStyle {
    fn default() -> Self {
        let [r, g, b, _] = crate::chrome::GROUP.to_array();
        Self {
            background: [r, g, b],
            text: [0, 0, 0],
            font_size: 8.,
            bold: true,
            row_height: 18.,
        }
    }
}

/// Band row pitch (cell plus its 1 px grid line) for a band font size in
/// points. Measured: 8 and 9 pt bands are 18 px like activity rows, 11 pt
/// bands 22 px and 12 pt bands 24 px, which is one and a half times the
/// font's pixel size.
pub fn band_row_height(font_size: f32) -> f32 {
    (font_size * 4. / 3. * 1.5).round().max(18.)
}

/// Default band styles for levels 1 to 17: the theme's navy and light blue
/// family (`crate::theme::BANDS`).
pub fn default_bands() -> Vec<BandStyle> {
    bands_from(&crate::theme::BANDS)
}

/// Band styles from `(background, text, font size)` rows given as `0xRRGGBB`.
pub fn bands_from(levels: &[(u32, u32, f32)]) -> Vec<BandStyle> {
    levels
        .iter()
        .map(|&(background, text, font_size)| BandStyle {
            background: crate::theme::bytes(background),
            text: crate::theme::bytes(text),
            font_size,
            bold: true,
            row_height: band_row_height(font_size),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupSort {
    pub levels: Vec<GroupLevel>,
    pub show_group_totals: bool,
    pub show_grand_totals: bool,
    pub show_summaries_only: bool,
    pub hide_if_empty: bool,
    pub sort_bands_alphabetically: bool,
    pub show_title: bool,
    pub show_code: bool,
    pub show_name: bool,
    pub sort: Vec<SortKey>,
    /// Styles for band levels 1 to 17.
    pub bands: Vec<BandStyle>,
    /// Band levels (from 0) that start a new printed page.
    pub page_breaks: Vec<usize>,
}

impl Default for GroupSort {
    fn default() -> Self {
        Self {
            levels: vec![GroupLevel {
                column: "wbs".into(),
                to_level: None,
                indent: true,
                interval: String::new(),
            }],
            show_group_totals: true,
            show_grand_totals: false,
            show_summaries_only: false,
            // Off in the reference; the legacy defaults of tests keep it on.
            hide_if_empty: !crate::defaults::reference(),
            sort_bands_alphabetically: false,
            show_title: false,
            show_code: true,
            show_name: true,
            sort: vec![SortKey {
                column: "activity_id".into(),
                descending: false,
            }],
            bands: default_bands(),
            page_breaks: vec![],
        }
    }
}

impl GroupSort {
    pub fn band_style(&self, level: usize) -> BandStyle {
        self.bands
            .get(level.min(MAX_BAND_LEVELS - 1))
            .cloned()
            .unwrap_or_default()
    }

    /// Pitch of a band row at `level`: the stored height, never less than
    /// the band font needs.
    pub fn band_height(&self, level: usize) -> f32 {
        let style = self.band_style(level);
        style.row_height.max(band_row_height(style.font_size))
    }
}

/// The activity table's font (Table Font and Row).
#[derive(Clone, Debug, PartialEq)]
pub struct TableFont {
    pub face: crate::chrome::Face,
    pub points: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: [u8; 3],
}

impl Default for TableFont {
    /// Arial 8 pt regular, black.
    fn default() -> Self {
        Self {
            face: crate::chrome::Face::Arial,
            points: 8.,
            bold: false,
            italic: false,
            color: [0, 0, 0],
        }
    }
}

impl TableFont {
    /// The plain table font the measured cell metrics were taken with.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// How the activity table draws its rows: Table Font and Row, Line Numbers
/// and Hint Help.
#[derive(Clone, Debug, PartialEq)]
pub struct TableDisplay {
    /// A row-number gutter at the left of the table.
    pub line_numbers: bool,
    /// Hovering a truncated cell shows its whole value.
    pub hint_help: bool,
    pub font: TableFont,
    /// Fill of the activity cells.
    pub cell_color: [u8; 3],
    /// Optimize height by row content: `Some(most lines per row)`, where 0
    /// is no limit; `None` gives every row the same height.
    pub optimize_rows: Option<u8>,
    /// A small activity type glyph before the first column's text.
    pub show_icons: bool,
}

impl Default for TableDisplay {
    fn default() -> Self {
        Self {
            line_numbers: false,
            hint_help: false,
            font: TableFont::default(),
            cell_color: [255, 255, 255],
            optimize_rows: None,
            // On in the reference; the legacy defaults of tests keep it off.
            show_icons: crate::defaults::reference(),
        }
    }
}

impl TableDisplay {
    /// What a stored layout without table options shows: the options as
    /// they were before the layout kept them (no icons).
    fn before_stored() -> Self {
        Self {
            show_icons: false,
            ..Self::default()
        }
    }
}

/// Activity row pitch a table font needs: 18 px up to 9 pt, then one and
/// a half times the font's pixel size, like band rows.
pub fn font_row_height(points: f32) -> f32 {
    band_row_height(points)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bottom {
    pub show_details: bool,
    pub tab: String,
    /// The bottom view (`panes::BottomView` key); empty for Activity Details.
    pub view: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    pub columns: Vec<ColumnSetting>,
    pub filter: FilterSelection,
    pub group_sort: GroupSort,
    /// The Gantt view settings (`activities_view_v1` form) saved with the layout.
    pub gantt: Option<Json>,
    pub bottom: Option<Bottom>,
    /// Pane options saved with the layout (top view, Trace Logic and
    /// Activity Network options), in the `panes::PanesState` form.
    pub panes: Option<Json>,
    /// Table font, rows, line numbers and hints.
    pub table: TableDisplay,
}

/// The default column set, as the reference starts: Activity ID, Activity
/// Name, Original Duration, Remaining Duration, Schedule % Complete, Start,
/// Finish, Total Float. Dates show no time by default, so the date columns
/// are narrower. The legacy defaults of tests (`crate::defaults`) are
/// `earlier_columns`.
pub fn default_columns() -> Vec<ColumnSetting> {
    if !crate::defaults::reference() {
        return earlier_columns();
    }
    columns_of(&[
        ("activity_id", 90.),
        ("activity_name", 250.),
        ("original_duration", 70.),
        ("remaining_duration", 70.),
        ("schedule_percent_complete", 70.),
        ("start", 75.),
        ("finish", 75.),
        ("total_float", 65.),
    ])
}

/// The column set of earlier builds: Activity ID, Activity Name, Activity
/// Status, Remaining Duration, Start, Finish, Total Float, Free Float. The
/// view preference of the oldest builds stored its order and widths.
pub fn earlier_columns() -> Vec<ColumnSetting> {
    columns_of(&[
        ("activity_id", 90.),
        ("activity_name", 250.),
        ("activity_status", 85.),
        ("remaining_duration", 70.),
        ("start", 100.),
        ("finish", 100.),
        ("total_float", 65.),
        ("free_float", 65.),
    ])
}

fn columns_of(list: &[(&str, f32)]) -> Vec<ColumnSetting> {
    list.iter()
        .map(|&(id, w)| ColumnSetting::new(id, w))
        .collect()
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            name: STANDARD_NAME.into(),
            columns: default_columns(),
            filter: FilterSelection::default(),
            group_sort: GroupSort::default(),
            gantt: None,
            bottom: None,
            panes: None,
            table: TableDisplay::default(),
        }
    }
}

fn color_hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}
fn parse_color(v: &Json) -> Result<[u8; 3], String> {
    let s = v.as_str().ok_or("color must be text")?;
    let hex = s.strip_prefix('#').ok_or("color must start with #")?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("bad color {s:?}"));
    }
    let byte = |n: usize| u8::from_str_radix(&hex[n..n + 2], 16).unwrap_or(0);
    Ok([byte(0), byte(2), byte(4)])
}
fn text(v: &Json, what: &str) -> Result<String, String> {
    v.as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{what} must be text"))
}
fn flag(v: &Json, key: &str, default: bool) -> Result<bool, String> {
    match &v[key] {
        Json::Null => Ok(default),
        Json::Bool(b) => Ok(*b),
        _ => Err(format!("{key} must be true or false")),
    }
}
fn number(v: &Json, key: &str, range: std::ops::RangeInclusive<f32>) -> Result<f32, String> {
    let n = v[key]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{key} must be a number"))? as f32;
    if range.contains(&n) {
        Ok(n)
    } else {
        Err(format!(
            "{key} {n} is outside {}..{}",
            range.start(),
            range.end()
        ))
    }
}
fn array<'a>(v: &'a Json, key: &str) -> Result<&'a Vec<Json>, String> {
    match &v[key] {
        Json::Array(a) => Ok(a),
        _ => Err(format!("{key} must be a list")),
    }
}

pub fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a layout name.".into());
    }
    if name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err("Layout names are up to 100 characters on one line.".into());
    }
    Ok(name.into())
}

impl Layout {
    pub fn to_json(&self) -> Json {
        let gs = &self.group_sort;
        json!({
            "name": self.name,
            "columns": self.columns.iter().map(|c| json!({
                "id": c.id, "width": c.width, "title": c.title,
                "align": c.align.map(Align::key),
            })).collect::<Vec<_>>(),
            "filter": {
                "all_activities": self.filter.all_activities,
                "selected": self.filter.selected,
                "match_all": self.filter.match_all,
                "highlight": self.filter.highlight,
            },
            "group_sort": {
                "levels": gs.levels.iter().map(|l| json!({
                    "column": l.column, "to_level": l.to_level, "indent": l.indent,
                    "interval": l.interval,
                })).collect::<Vec<_>>(),
                "page_breaks": gs.page_breaks,
                "show_group_totals": gs.show_group_totals,
                "show_grand_totals": gs.show_grand_totals,
                "show_summaries_only": gs.show_summaries_only,
                "hide_if_empty": gs.hide_if_empty,
                "sort_bands_alphabetically": gs.sort_bands_alphabetically,
                "show_title": gs.show_title,
                "show_code": gs.show_code,
                "show_name": gs.show_name,
                "sort": gs.sort.iter().map(|s| json!({
                    "column": s.column, "descending": s.descending,
                })).collect::<Vec<_>>(),
                "bands": gs.bands.iter().map(|b| json!({
                    "background": color_hex(b.background), "text": color_hex(b.text),
                    "font_size": b.font_size, "bold": b.bold, "row_height": b.row_height,
                })).collect::<Vec<_>>(),
            },
            "gantt": self.gantt,
            "bottom": self.bottom.as_ref().map(|b| json!({
                "show_details": b.show_details, "tab": b.tab, "view": b.view,
            })),
            "panes": self.panes,
            "table": table_to_json(&self.table),
        })
    }

    /// Parse and validate a stored layout. Any invalid part rejects the whole.
    pub fn from_json(v: &Json) -> Result<Self, String> {
        if !v.is_object() {
            return Err("a layout must be an object".into());
        }
        let name = check_name(&text(&v["name"], "name")?)?;
        let mut columns = vec![];
        for c in array(v, "columns")? {
            let id = text(&c["id"], "column id")?;
            if id.trim().is_empty() || id.len() > 200 {
                return Err("column ids must be 1 to 200 characters".into());
            }
            if columns.iter().any(|x: &ColumnSetting| x.id == id) {
                return Err(format!("column {id:?} is listed twice"));
            }
            let title = match &c["title"] {
                Json::Null => None,
                t => Some(text(t, "column title")?),
            };
            let align = match &c["align"] {
                Json::Null => None,
                a => Some(
                    Align::from_key(a.as_str().unwrap_or(""))
                        .ok_or("align must be left, center or right")?,
                ),
            };
            columns.push(ColumnSetting {
                id,
                width: number(c, "width", 8. ..=2000.)?,
                title,
                align,
            });
        }
        if columns.is_empty() {
            return Err("a layout needs at least one column".into());
        }
        let f = &v["filter"];
        if !f.is_object() {
            return Err("filter must be an object".into());
        }
        let filter = FilterSelection {
            all_activities: flag(f, "all_activities", true)?,
            selected: array(f, "selected")?
                .iter()
                .map(|k| text(k, "filter key"))
                .collect::<Result<_, _>>()?,
            match_all: flag(f, "match_all", true)?,
            highlight: flag(f, "highlight", false)?,
        };
        let g = &v["group_sort"];
        if !g.is_object() {
            return Err("group_sort must be an object".into());
        }
        let mut levels = vec![];
        for l in array(g, "levels")? {
            let to_level = match &l["to_level"] {
                Json::Null => None,
                n => Some(
                    n.as_u64()
                        .filter(|n| (1..=50).contains(n))
                        .ok_or("to_level must be 1 to 50 or null")? as usize,
                ),
            };
            let interval = match &l["interval"] {
                // Layouts saved before Group Interval existed.
                Json::Null => String::new(),
                i => text(i, "group interval")?.trim().to_string(),
            };
            if interval.chars().count() > 30 || interval.chars().any(char::is_control) {
                return Err("a group interval is up to 30 characters".into());
            }
            levels.push(GroupLevel {
                column: text(&l["column"], "group column")?,
                to_level,
                indent: flag(l, "indent", true)?,
                interval,
            });
        }
        let mut sort = vec![];
        for s in array(g, "sort")? {
            sort.push(SortKey {
                column: text(&s["column"], "sort column")?,
                descending: flag(s, "descending", false)?,
            });
        }
        let mut bands = vec![];
        for b in array(g, "bands")? {
            bands.push(BandStyle {
                background: parse_color(&b["background"])?,
                text: parse_color(&b["text"])?,
                font_size: number(b, "font_size", 6. ..=24.)?,
                bold: flag(b, "bold", true)?,
                row_height: number(b, "row_height", 12. ..=60.)?,
            });
        }
        if bands.len() > MAX_BAND_LEVELS {
            return Err(format!("at most {MAX_BAND_LEVELS} band styles"));
        }
        // Layouts saved before the measured band styles existed hold the
        // single gray style on every level; they get the defaults.
        if bands.iter().all(|b| *b == BandStyle::default()) {
            bands.clear();
        }
        let fill = default_bands();
        let stored = bands.len();
        bands.extend_from_slice(&fill[stored..]);
        let mut page_breaks = vec![];
        if !g["page_breaks"].is_null() {
            for n in array(g, "page_breaks")? {
                let n = n
                    .as_u64()
                    .filter(|n| (*n as usize) < MAX_BAND_LEVELS)
                    .ok_or_else(|| {
                        format!("page breaks are band levels 0 to {}", MAX_BAND_LEVELS - 1)
                    })? as usize;
                if page_breaks.contains(&n) {
                    return Err(format!("page break {n} is listed twice"));
                }
                page_breaks.push(n);
            }
            page_breaks.sort_unstable();
        }
        let defaults = GroupSort::default();
        let group_sort = GroupSort {
            levels,
            show_group_totals: flag(g, "show_group_totals", defaults.show_group_totals)?,
            show_grand_totals: flag(g, "show_grand_totals", false)?,
            show_summaries_only: flag(g, "show_summaries_only", false)?,
            hide_if_empty: flag(g, "hide_if_empty", true)?,
            sort_bands_alphabetically: flag(g, "sort_bands_alphabetically", false)?,
            show_title: flag(g, "show_title", false)?,
            show_code: flag(g, "show_code", defaults.show_code)?,
            show_name: flag(g, "show_name", true)?,
            sort,
            bands,
            page_breaks,
        };
        let gantt = match &v["gantt"] {
            Json::Null => None,
            Json::Object(_) => Some(v["gantt"].clone()),
            _ => return Err("gantt must be an object".into()),
        };
        let bottom = match &v["bottom"] {
            Json::Null => None,
            b => {
                let tab = text(&b["tab"], "bottom tab")?;
                let view = match &b["view"] {
                    // Trace Logic was a details tab in older layouts.
                    Json::Null if tab == "trace_logic" => tab.clone(),
                    Json::Null => String::new(),
                    v => text(v, "bottom view")?,
                };
                Some(Bottom {
                    show_details: flag(b, "show_details", true)?,
                    tab,
                    view,
                })
            }
        };
        let panes = match &v["panes"] {
            Json::Null => None,
            Json::Object(_) => Some(v["panes"].clone()),
            _ => return Err("panes must be an object".into()),
        };
        let table = match &v["table"] {
            // Layouts saved before these options existed.
            Json::Null => TableDisplay::before_stored(),
            t => table_from_json(t)?,
        };
        Ok(Self {
            name,
            columns,
            filter,
            group_sort,
            gantt,
            bottom,
            panes,
            table,
        })
    }

    /// The layout without the parts that change while working (Gantt zoom),
    /// used to decide whether it differs from its saved copy.
    pub fn content_key(&self) -> String {
        let mut v = self.to_json();
        v["gantt"] = Json::Null;
        v.to_string()
    }
}

pub(crate) fn table_to_json(t: &TableDisplay) -> Json {
    json!({
        "line_numbers": t.line_numbers,
        "hint_help": t.hint_help,
        "font": {
            "face": t.font.face.name(),
            "points": t.font.points,
            "bold": t.font.bold,
            "italic": t.font.italic,
            "color": color_hex(t.font.color),
        },
        "cell_color": color_hex(t.cell_color),
        "optimize_rows": t.optimize_rows,
        "show_icons": t.show_icons,
    })
}

pub(crate) fn table_from_json(t: &Json) -> Result<TableDisplay, String> {
    if !t.is_object() {
        return Err("table must be an object".into());
    }
    let f = &t["font"];
    if !f.is_object() {
        return Err("table font must be an object".into());
    }
    let face = text(&f["face"], "table font face")?;
    let face = crate::chrome::Face::from_name(&face)
        .ok_or_else(|| format!("unknown table font {face:?}"))?;
    let optimize_rows = match &t["optimize_rows"] {
        Json::Null => None,
        n => Some(
            n.as_u64()
                .filter(|n| *n <= 20)
                .ok_or("optimize_rows must be 0 to 20 or null")? as u8,
        ),
    };
    Ok(TableDisplay {
        line_numbers: flag(t, "line_numbers", false)?,
        hint_help: flag(t, "hint_help", false)?,
        font: TableFont {
            face,
            points: number(f, "points", 6. ..=20.)?,
            bold: flag(f, "bold", false)?,
            italic: flag(f, "italic", false)?,
            color: parse_color(&f["color"])?,
        },
        cell_color: parse_color(&t["cell_color"])?,
        optimize_rows,
        show_icons: flag(t, "show_icons", false)?,
    })
}

/// Version of the layout file format written by Export.
pub const FILE_VERSION: u64 = 1;
/// Marker naming the layout file format.
pub const FILE_FORMAT: &str = "openxer-layout";
/// Largest layout file Import reads.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// A layout file: the format marker, its version and the layout.
pub fn file_text(layout: &Layout) -> String {
    let v = json!({
        "format": FILE_FORMAT,
        "version": FILE_VERSION,
        "layout": layout.to_json(),
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// Parse and validate a layout file. The whole file is refused when it is
/// too large, not this format, a newer version, or any part is invalid.
pub fn from_file_text(text: &str) -> Result<Layout, String> {
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "The layout file is larger than {} MB.",
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    let v: Json =
        serde_json::from_str(text).map_err(|e| format!("The file is not a layout file: {e}"))?;
    if v["format"].as_str() != Some(FILE_FORMAT) {
        return Err("The file is not a layout file.".into());
    }
    match v["version"].as_u64() {
        Some(n) if (1..=FILE_VERSION).contains(&n) => {}
        Some(n) => return Err(format!("Layout file version {n} is not supported.")),
        None => return Err("The layout file has no version.".into()),
    }
    let layout = Layout::from_json(&v["layout"]).map_err(|e| format!("Invalid layout: {e}"))?;
    if layout.columns.len() > 400 {
        return Err("Invalid layout: more than 400 columns.".into());
    }
    if layout.group_sort.levels.len() > MAX_BAND_LEVELS {
        return Err("Invalid layout: too many grouping levels.".into());
    }
    Ok(layout)
}

/// A list of layouts: each must be valid and names unique, or none load.
pub fn library_from_json(text: &str) -> Result<Vec<Layout>, String> {
    let v: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Json::Array(items) = v else {
        return Err("layouts must be a list".into());
    };
    let mut out: Vec<Layout> = vec![];
    for item in &items {
        let layout = Layout::from_json(item)?;
        if out
            .iter()
            .any(|l| l.name.eq_ignore_ascii_case(&layout.name))
        {
            return Err(format!("layout {:?} is listed twice", layout.name));
        }
        out.push(layout);
    }
    Ok(out)
}

pub fn library_to_json(layouts: &[Layout]) -> String {
    Json::Array(layouts.iter().map(Layout::to_json).collect()).to_string()
}

pub fn filter_to_json(f: &UserFilter) -> Json {
    json!({
        "name": f.name, "match_all": f.match_all,
        "rows": f.rows.iter().map(|r| json!({
            "column": r.column, "op": r.op.key(), "value": r.value, "high": r.high,
            "level": r.level,
        })).collect::<Vec<_>>(),
    })
}

pub fn filter_from_json(v: &Json) -> Result<UserFilter, String> {
    let name = text(&v["name"], "filter name")?.trim().to_string();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("filter names are 1 to 100 characters".into());
    }
    let mut rows = vec![];
    for r in array(v, "rows")? {
        let level = match &r["level"] {
            // Filters saved before nested groups existed.
            Json::Null => 0,
            n => n
                .as_u64()
                .filter(|n| *n as usize <= MAX_LEVEL)
                .ok_or_else(|| format!("filter row levels are 0 to {MAX_LEVEL}"))?
                as usize,
        };
        let column = text(&r["column"], "parameter")?;
        if column.len() > 200 {
            return Err("filter parameters are up to 200 characters".into());
        }
        rows.push(Criterion {
            column,
            op: Op::from_key(r["op"].as_str().unwrap_or("")).ok_or("unknown filter operator")?,
            value: text(&r["value"], "value")?,
            high: text(&r["high"], "high value")?,
            level,
        });
    }
    if rows.len() > 500 {
        return Err("a filter holds at most 500 rows".into());
    }
    Ok(UserFilter {
        name,
        match_all: flag(v, "match_all", true)?,
        rows,
    })
}

pub fn filters_from_json(text: &str) -> Result<Vec<UserFilter>, String> {
    let v: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Json::Array(items) = v else {
        return Err("filters must be a list".into());
    };
    let mut out: Vec<UserFilter> = vec![];
    for item in &items {
        let f = filter_from_json(item)?;
        if out.iter().any(|x| x.name == f.name) {
            return Err(format!("filter {:?} is listed twice", f.name));
        }
        out.push(f);
    }
    Ok(out)
}

pub fn filters_to_json(filters: &[UserFilter]) -> String {
    Json::Array(filters.iter().map(filter_to_json).collect()).to_string()
}

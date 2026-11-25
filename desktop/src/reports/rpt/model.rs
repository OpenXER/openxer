//! The report model read from a definition stream: page settings plus
//! sections; a section loops over a data source (optionally as a tree) and
//! holds rows; a row holds cells (text, data field or variable, line, image).
//! The stream stays the stored form; this view is rebuilt after every edit.
use super::filter::{self, Filter, SortKey};
use super::page::{self, Page};
use super::stream::{Component, Item, Stream};

/// Group nesting read from a definition; deeper components are an error.
pub const MAX_NEST: usize = 16;

pub const REPORT: &str = "TCVReport";
pub const ROW_CONTAINER: &str = "TCVReportRowContainer";
pub const SECTION: &str = "TCVReportSectionContainer";
pub const GROUP_BY: &str = "TCVReportGroupBy";
pub const RECURSE: &str = "TCVReportGroupByTreeRecurse";
pub const ROW: &str = "TCVReportRow";
pub const TEXT_CELL: &str = "TCVReportTextCell";
pub const LINE_CELL: &str = "TCVReportLineCell";
pub const IMAGE_CELL: &str = "TCVReportImageCell";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    /// A color: a `cl` name, `$00BBGGRR` or a decimal number.
    pub fn parse(text: &str) -> Option<Rgb> {
        let t = text.trim();
        let named = match t {
            "clBlack" => 0x000000,
            "clMaroon" => 0x000080,
            "clGreen" => 0x008000,
            "clOlive" => 0x008080,
            "clNavy" => 0x800000,
            "clPurple" => 0x800080,
            "clTeal" => 0x808000,
            "clGray" => 0x808080,
            "clSilver" => 0xC0C0C0,
            "clRed" => 0x0000FF,
            "clLime" => 0x00FF00,
            "clYellow" => 0x00FFFF,
            "clBlue" => 0xFF0000,
            "clFuchsia" => 0xFF00FF,
            "clAqua" => 0xFFFF00,
            "clLtGray" => 0xC0C0C0,
            "clDkGray" => 0x808080,
            "clWhite" => 0xFFFFFF,
            "clWindow" => 0xFFFFFF,
            "clBtnFace" => 0xF0F0F0,
            "clWindowText" => 0x000000,
            _ => {
                let v = if let Some(h) = t.strip_prefix('$') {
                    u32::from_str_radix(h, 16).ok()?
                } else {
                    t.parse::<i64>()
                        .ok()
                        .and_then(|v| u32::try_from(v & 0xFF_FFFF).ok())?
                };
                v & 0xFF_FFFF
            }
        };
        Some(Rgb(
            (named & 0xFF) as u8,
            ((named >> 8) & 0xFF) as u8,
            ((named >> 16) & 0xFF) as u8,
        ))
    }

    #[cfg(test)]
    /// The color as the definition text (`$00BBGGRR`, or a name for the
    /// common ones).
    pub fn text(self) -> String {
        match self {
            Rgb(0, 0, 0) => "clBlack".into(),
            Rgb(255, 255, 255) => "clWhite".into(),
            Rgb(r, g, b) => format!("${:02X}{:02X}{:02X}{:02X}", 0, b, g, r),
        }
    }

    pub fn css(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

impl Align {
    pub fn parse(text: &str) -> Option<Align> {
        match text.trim() {
            "taLeftJustify" | "caLeft" => Some(Align::Left),
            "taRightJustify" | "caRight" => Some(Align::Right),
            "taCenter" | "caCenter" => Some(Align::Center),
            _ => None,
        }
    }
    #[cfg(test)]
    pub fn text(self) -> &'static str {
        match self {
            Align::Left => "taLeftJustify",
            Align::Right => "taRightJustify",
            Align::Center => "taCenter",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub name: String,
    /// Height of the em in pixels at 96 per inch.
    pub px: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Rgb,
}

impl Default for Font {
    fn default() -> Self {
        Font {
            name: "Arial".into(),
            px: 13.,
            bold: false,
            italic: false,
            underline: false,
            color: Rgb::BLACK,
        }
    }
}

impl Font {
    /// Reads `<prefix>.Name`, `.Height`, `.Style` and `.Color`.
    fn read(c: &Component, prefix: &str, base: &Font) -> Font {
        let mut f = base.clone();
        if let Some(n) = c.string(&format!("{prefix}.Name")) {
            f.name = n;
        }
        if let Some(h) = c.int(&format!("{prefix}.Height")) {
            if h != 0 {
                f.px = (h.unsigned_abs() as f32).clamp(4., 200.);
            }
        }
        if let Some(s) = c.raw(&format!("{prefix}.Style")) {
            f.bold = s.contains("fsBold");
            f.italic = s.contains("fsItalic");
            f.underline = s.contains("fsUnderline");
        }
        if let Some(col) = c.raw(&format!("{prefix}.Color")).and_then(Rgb::parse) {
            f.color = col;
        }
        f
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Borders {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl Borders {
    fn parse(text: &str) -> Borders {
        Borders {
            left: text.contains("rpLeft"),
            top: text.contains("rpTop"),
            right: text.contains("rpRight"),
            bottom: text.contains("rpBottom"),
        }
    }
    #[cfg(test)]
    pub fn text(self) -> String {
        let mut v = vec![];
        for (on, name) in [
            (self.left, "rpLeft"),
            (self.top, "rpTop"),
            (self.right, "rpRight"),
            (self.bottom, "rpBottom"),
        ] {
            if on {
                v.push(name);
            }
        }
        format!("[{}]", v.join(", "))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Aggregate {
    #[default]
    Sum,
    Min,
    Max,
    Average,
    Count,
}

impl Aggregate {
    pub fn parse(text: &str) -> Aggregate {
        match text.trim() {
            "amMin" => Aggregate::Min,
            "amMax" => Aggregate::Max,
            "amAvg" | "amAverage" => Aggregate::Average,
            "amCount" => Aggregate::Count,
            _ => Aggregate::Sum,
        }
    }
    pub fn text(self) -> &'static str {
        match self {
            Aggregate::Sum => "amSum",
            Aggregate::Min => "amMin",
            Aggregate::Max => "amMax",
            Aggregate::Average => "amAvg",
            Aggregate::Count => "amCount",
        }
    }
    pub const ALL: [Aggregate; 5] = [
        Aggregate::Sum,
        Aggregate::Min,
        Aggregate::Max,
        Aggregate::Average,
        Aggregate::Count,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            Aggregate::Sum => "Sum",
            Aggregate::Min => "Minimum",
            Aggregate::Max => "Maximum",
            Aggregate::Average => "Average",
            Aggregate::Count => "Count",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// Fixed text.
    Text(String),
    /// Text with `[variable]` placeholders.
    Variable(String),
    /// A data field of the current record.
    Field(String),
    /// The title of a data field (its own title when set).
    Title {
        field: String,
        title: String,
    },
    /// An aggregate of a field over the records below the current one.
    Summary {
        field: String,
        path: String,
        mode: Aggregate,
    },
    Line {
        count: u32,
    },
    Image {
        file: String,
    },
}

/// A cell that repeats across the report's date range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spread;

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub left: f32,
    pub width: f32,
    pub height: f32,
    pub align: Option<Align>,
    pub background: Option<Rgb>,
    pub font: Font,
    pub borders: Borders,
    pub content: Content,
    pub hide_if_empty: bool,
    pub show_indent: bool,
    pub spread: Option<Spread>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub height: f32,
    pub filter: Filter,
    pub hide_if_empty: bool,
    pub background: Option<Rgb>,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Row(Row),
    Group(Box<Group>),
    /// Where a tree group continues with the children of the current node.
    Recurse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageBreak {
    #[default]
    None,
    AfterRecord,
    AfterSet,
}

/// A section's loop over a data source.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The data source (table) looped over.
    pub table: String,
    /// The path from the enclosing record, when joined to it.
    pub join: Option<String>,
    /// An inner join leaves out enclosing records with none of these.
    pub inner: bool,
    pub tree: bool,
    /// Deepest tree level shown; 0 for all.
    pub max_levels: u32,
    pub filter: Filter,
    pub sort: Vec<SortKey>,
    pub hide_if_no_children: bool,
    pub hide_if_empty: bool,
    pub page_break: PageBreak,
    pub header: Vec<Row>,
    pub body: Vec<Node>,
    pub footer: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub title: String,
    pub name: String,
    pub page: Page,
    pub variables: Vec<String>,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub date_units: String,
    pub report_header: Vec<Row>,
    pub page_header: Vec<Row>,
    pub body: Vec<Node>,
    pub page_footer: Vec<Row>,
    pub report_footer: Vec<Row>,
}

fn container_type(c: &Component) -> String {
    c.raw("ContainerType").unwrap_or("").trim().to_string()
}

impl Report {
    pub fn from_stream(s: &Stream) -> Result<Report, String> {
        let root = &s.root;
        if root.class != REPORT {
            return Err(format!("This is a {} definition, not a report", root.class));
        }
        let page = root
            .list(page::PAYLOAD_PROP)
            .as_deref()
            .and_then(page::read_payload)
            .map(|t| Page::from_tree(&t))
            .unwrap_or_default();
        let base = Font::default();
        let styles = Styles::read(root, &base);
        let mut r = Report {
            title: root.string("Title").unwrap_or_default(),
            name: root.string("ReportName").unwrap_or_default(),
            page,
            variables: root.list("Variables.Strings").unwrap_or_default(),
            start_date: root.string("StartDate"),
            finish_date: root.string("FinishDate"),
            date_units: root.raw("DateUnits").unwrap_or("utWeek").trim().to_string(),
            report_header: vec![],
            page_header: vec![],
            body: vec![],
            page_footer: vec![],
            report_footer: vec![],
        };
        for child in root.children() {
            match (child.class.as_str(), container_type(child).as_str()) {
                (ROW_CONTAINER, "rctReportHeader") => r.report_header.extend(rows(child, &styles)?),
                (ROW_CONTAINER, "rctPageHeader") => r.page_header.extend(rows(child, &styles)?),
                (ROW_CONTAINER, "rctPageFooter") => r.page_footer.extend(rows(child, &styles)?),
                (ROW_CONTAINER, "rctReportFooter") => r.report_footer.extend(rows(child, &styles)?),
                (SECTION, _) => r.body.extend(nodes(child, &styles, 0)?),
                _ => {}
            }
        }
        Ok(r)
    }
}

/// The named font styles of the report (`FontHeading1` ... `FontBody3`).
struct Styles {
    list: Vec<(String, Font)>,
    base: Font,
}

impl Styles {
    fn read(root: &Component, base: &Font) -> Styles {
        let mut list = vec![];
        for (key, name, px, bold) in [
            ("rfsHeading1", "FontHeading1", 19., true),
            ("rfsHeading2", "FontHeading2", 16., true),
            ("rfsHeading3", "FontHeading3", 13., true),
            ("rfsHeading4", "FontHeading4", 11., true),
            ("rfsBody1", "FontBody1", 19., false),
            ("rfsBody2", "FontBody2", 16., false),
            ("rfsBody3", "FontBody3", 13., false),
        ] {
            let seed = Font {
                px,
                bold,
                ..base.clone()
            };
            list.push((key.to_string(), Font::read(root, name, &seed)));
        }
        Styles {
            list,
            base: base.clone(),
        }
    }
    fn get(&self, key: &str) -> Font {
        self.list
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| self.base.clone(), |(_, f)| f.clone())
    }
}

/// Items in layout order: by `Top` when every item has one, else as written.
fn in_layout_order<T>(mut items: Vec<(Option<i64>, T)>) -> Vec<T> {
    if items.iter().all(|(top, _)| top.is_some()) {
        items.sort_by_key(|(top, _)| *top);
    }
    items.into_iter().map(|(_, t)| t).collect()
}

fn rows(container: &Component, styles: &Styles) -> Result<Vec<Row>, String> {
    let items = container
        .children()
        .filter(|c| c.class == ROW)
        .map(|c| (c.int("Top"), row(c, styles)))
        .collect();
    Ok(in_layout_order(items))
}

fn row(c: &Component, styles: &Styles) -> Row {
    let mut cells = vec![];
    for k in c.children() {
        if let Some(cell) = cell(k, styles) {
            cells.push(cell);
        }
    }
    Row {
        height: c.int("Height").unwrap_or(20).clamp(1, 2000) as f32,
        filter: c
            .list("Filter.Strings")
            .map_or(Filter::And(vec![]), |l| filter::parse(&l)),
        hide_if_empty: c.flag("HideIfEmpty").unwrap_or(false),
        background: c
            .raw("Color")
            .and_then(Rgb::parse)
            .filter(|c| *c != Rgb::WHITE),
        cells,
    }
}

fn cell(c: &Component, styles: &Styles) -> Option<Cell> {
    let left = c.int("Left").unwrap_or(0) as f32;
    let width = c.int("Width").unwrap_or(100).clamp(0, 20000) as f32;
    let height = c.int("Height").unwrap_or(19).clamp(1, 2000) as f32;
    let background = c
        .raw("Color")
        .and_then(Rgb::parse)
        .filter(|c| *c != Rgb::WHITE);
    let borders = c.raw("Borders").map(Borders::parse).unwrap_or_default();
    let mut base = Cell {
        left,
        width,
        height,
        align: None,
        background,
        font: Font::default(),
        borders,
        content: Content::Text(String::new()),
        hide_if_empty: c.flag("HideIfEmpty").unwrap_or(false),
        show_indent: c.flag("ShowIndent").unwrap_or(false),
        spread: c.flag("ShowTimeDistrib").unwrap_or(false).then_some(Spread),
    };
    match c.class.as_str() {
        LINE_CELL => {
            base.content = Content::Line {
                count: c.int("NumLines").unwrap_or(1).clamp(1, 20) as u32,
            };
            Some(base)
        }
        IMAGE_CELL => {
            base.content = Content::Image {
                file: c.string("ImageFileName").unwrap_or_default(),
            };
            Some(base)
        }
        TEXT_CELL => {
            let style = c.raw("FontStyle").unwrap_or("rfsBody3").trim().to_string();
            let from_style = styles.get(&style);
            base.font = if style == "rfsCustom" || c.prop("Font.Name").is_some() {
                Font::read(c, "Font", &from_style)
            } else {
                from_style
            };
            base.align = c.raw("Alignment").and_then(Align::parse);
            let field = c.string("DataField").unwrap_or_default();
            base.content = match c.raw("CellType").unwrap_or("").trim() {
                "ctCustomText" => Content::Text(c.string("Text").unwrap_or_default()),
                "ctVariable" => Content::Variable(c.string("VariableField").unwrap_or_default()),
                "ctFieldTitle" => Content::Title {
                    title: c.string("FieldTitle").unwrap_or_default(),
                    field,
                },
                "ctFieldSummary" => Content::Summary {
                    field,
                    path: c.string("SummaryGroupByPath").unwrap_or_default(),
                    mode: c.raw("AggrMode").map(Aggregate::parse).unwrap_or_default(),
                },
                _ => Content::Field(field),
            };
            Some(base)
        }
        _ => None,
    }
}

/// The rows and groups of a section container, in order.
fn nodes(section: &Component, styles: &Styles, depth: usize) -> Result<Vec<Node>, String> {
    if depth > MAX_NEST {
        return Err("The report nests sections too deeply".into());
    }
    let mut out = vec![];
    for item in &section.items {
        let Item::Child(c) = item else { continue };
        let node = match c.class.as_str() {
            ROW => Node::Row(row(c, styles)),
            GROUP_BY => Node::Group(Box::new(group(c, styles, depth + 1)?)),
            RECURSE => Node::Recurse,
            _ => continue,
        };
        out.push((c.int("Top"), node));
    }
    Ok(in_layout_order(out))
}

fn group(c: &Component, styles: &Styles, depth: usize) -> Result<Group, String> {
    let (inner_path, outer_path) = (
        c.string("GroupByInnerJoinPath"),
        c.string("GroupByOuterJoinPath"),
    );
    let join = inner_path.clone().or(outer_path);
    let table = c.string("GroupByTableName").unwrap_or_else(|| {
        join.as_deref()
            .and_then(|j| j.rsplit(['.', '-']).next())
            .unwrap_or("")
            .to_string()
    });
    let mut g = Group {
        table,
        join,
        inner: inner_path.is_some(),
        tree: c.raw("GroupByType").is_some_and(|t| t.trim() == "gtTree"),
        max_levels: c.int("MaxLevels").unwrap_or(0).clamp(0, 1000) as u32,
        filter: c
            .list("Filter.Strings")
            .map_or(Filter::And(vec![]), |l| filter::parse(&l)),
        sort: c
            .list("SortBy.Strings")
            .map(|l| filter::parse_sort(&l))
            .unwrap_or_default(),
        hide_if_no_children: c.flag("HideRowIfNoChildren").unwrap_or(false),
        hide_if_empty: c.flag("HideIfEmpty").unwrap_or(false),
        page_break: match c.raw("PageBreakType").map(str::trim) {
            Some("pbtAfterRecord") => PageBreak::AfterRecord,
            Some("pbtAfterSet") => PageBreak::AfterSet,
            _ => PageBreak::None,
        },
        header: vec![],
        body: vec![],
        footer: vec![],
    };
    for k in c.children() {
        match (k.class.as_str(), container_type(k).as_str()) {
            (ROW_CONTAINER, "rctGroupByHeader") => g.header.extend(rows(k, styles)?),
            (ROW_CONTAINER, "rctGroupbyFooter") => g.footer.extend(rows(k, styles)?),
            (SECTION, _) => g.body.extend(nodes(k, styles, depth)?),
            _ => {}
        }
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(Rgb::parse("clWhite"), Some(Rgb::WHITE));
        assert_eq!(Rgb::parse("$00FF8000"), Some(Rgb(0x00, 0x80, 0xFF)));
        assert_eq!(Rgb::parse("255"), Some(Rgb(255, 0, 0)));
        assert_eq!(Rgb::parse("junk"), None);
        for c in [Rgb(1, 2, 3), Rgb::BLACK, Rgb::WHITE] {
            assert_eq!(Rgb::parse(&c.text()), Some(c));
        }
    }

    #[test]
    fn borders_and_alignment() {
        let b = Borders::parse("[rpLeft, rpBottom]");
        assert!(b.left && b.bottom && !b.top && !b.right);
        assert_eq!(Borders::parse(&b.text()), b);
        assert_eq!(Align::parse("taRightJustify"), Some(Align::Right));
        assert_eq!(Align::parse(Align::Center.text()), Some(Align::Center));
    }
}

//! Properties of a report component: tabs for the report (title, page
//! setup, header and footer), a data source, a row, a text cell, a line and
//! an image. The dialog edits a copy of the definition; OK hands the copy
//! back to the editor, Cancel and Escape drop it.
use super::rpt::edit::{self, Kind, Path};
use super::rpt::model::{Aggregate, Rgb};
use super::rpt::page::{self, Orientation, Page};
use super::rpt::stream::{Component, Stream};
use super::rpt::{data, filter};
use crate::dialog_kit as kit;
use crate::schedule_dialogs::Canvas;
use eframe::egui::{self, Align, Vec2};
use openxer_xer::tree::Tree;
use std::collections::HashMap;

pub const TABS: [(&str, &str); 6] = [
    ("Report", "report"),
    ("Source", "source"),
    ("Row", "row"),
    ("Cell", "cell"),
    ("Line", "line"),
    ("Image", "image"),
];

pub struct Props {
    pub draft: Stream,
    /// The component the dialog was opened on.
    pub path: Path,
    pub tab: usize,
    pub bufs: HashMap<String, String>,
    page: Page,
    payload: Tree,
    pub error: Option<String>,
}

impl Props {
    pub fn new(stream: &Stream, path: Path) -> Props {
        let payload = stream
            .root
            .list(page::PAYLOAD_PROP)
            .as_deref()
            .and_then(page::read_payload)
            .unwrap_or_else(page::new_payload);
        let tab = match edit::get(&stream.root, &path).map(edit::kind) {
            Some(Kind::Group | Kind::GroupHeader | Kind::GroupFooter | Kind::Recurse) => 1,
            Some(Kind::Row) => 2,
            Some(Kind::Text) => 3,
            Some(Kind::Line) => 4,
            Some(Kind::Image) => 5,
            _ => 0,
        };
        Props {
            page: Page::from_tree(&payload),
            payload,
            draft: stream.clone(),
            path,
            tab,
            bufs: HashMap::new(),
            error: None,
        }
    }

    /// The path of the component a tab edits, when the selection has one.
    pub fn target(&self, tab: usize) -> Option<Path> {
        let root = &self.draft.root;
        let mut at = self.path.clone();
        // Walk up from the selection to the first component of the right kind.
        loop {
            let c = edit::get(root, &at)?;
            let k = edit::kind(c);
            let hit = match tab {
                0 => return Some(vec![]),
                1 => k == Kind::Group,
                2 => k == Kind::Row,
                3 => k == Kind::Text,
                4 => k == Kind::Line,
                _ => k == Kind::Image,
            };
            if hit {
                return Some(at);
            }
            // Cells only belong to the cell tabs; a row or source is above them.
            if matches!(tab, 3..=5) || at.is_empty() {
                return None;
            }
            at.pop();
        }
    }

    fn save_page(&mut self) {
        if self.page == Page::from_tree(&self.payload) {
            return;
        }
        self.page.apply(&mut self.payload);
        self.draft
            .root
            .set_list(page::PAYLOAD_PROP, &page::payload_lines(&self.payload));
    }
}

struct Form<'a, 'c> {
    c: &'a mut Canvas<'c>,
    bufs: &'a mut HashMap<String, String>,
    path: String,
}

const LABEL: f32 = 150.;

impl Form<'_, '_> {
    fn key(&self, prop: &str) -> String {
        format!(
            "props_{}",
            prop.to_ascii_lowercase().replace(['.', ' ', '|'], "_")
        )
    }

    fn buf(&mut self, prop: &str, initial: impl FnOnce() -> String) -> String {
        let k = format!("{}:{prop}", self.path);
        self.bufs.entry(k).or_insert_with(initial).clone()
    }

    fn put(&mut self, prop: &str, text: String) {
        self.bufs.insert(format!("{}:{prop}", self.path), text);
    }

    /// A quoted-string property.
    fn string(&mut self, x: f32, y: f32, w: f32, label: &str, comp: &mut Component, prop: &str) {
        kit::text(self.c, x, y + 3., label, true);
        let mut text = self.buf(prop, || comp.string(prop).unwrap_or_default());
        let key = self.key(prop);
        if self
            .c
            .edit(
                &key,
                x + LABEL,
                y,
                w - LABEL,
                21.,
                &mut text,
                true,
                Align::Min,
            )
            .changed()
        {
            comp.set_string(prop, &text);
            self.put(prop, text);
        }
    }

    /// An integer property.
    #[allow(clippy::too_many_arguments)]
    fn int(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        label: &str,
        comp: &mut Component,
        prop: &str,
        range: (i64, i64),
    ) {
        self.number(x, y, w, label, comp, prop, range, false);
    }

    /// An integer property; `negated` shows and takes the opposite sign
    /// (text heights are stored below zero).
    #[allow(clippy::too_many_arguments)]
    fn number(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        label: &str,
        comp: &mut Component,
        prop: &str,
        range: (i64, i64),
        negated: bool,
    ) {
        kit::text(self.c, x, y + 3., label, true);
        let sign = if negated { -1 } else { 1 };
        let mut text = self.buf(prop, || {
            comp.int(prop)
                .map(|v| (v * sign).to_string())
                .unwrap_or_default()
        });
        let key = self.key(prop);
        if self
            .c
            .edit(
                &key,
                x + LABEL,
                y,
                w - LABEL,
                21.,
                &mut text,
                true,
                Align::Min,
            )
            .changed()
        {
            if let Ok(v) = text.trim().parse::<i64>() {
                comp.set_int(prop, (v * sign).clamp(range.0, range.1));
            }
            self.put(prop, text);
        }
    }

    fn flag(&mut self, x: f32, y: f32, w: f32, label: &str, comp: &mut Component, prop: &str) {
        let mut on = comp.flag(prop).unwrap_or(false);
        let key = self.key(prop);
        if self
            .c
            .checkbox(&key, x, y, w, 18., label, &mut on, true)
            .clicked()
        {
            comp.set_flag(prop, on);
        }
    }

    /// A drop-down over raw values.
    #[allow(clippy::too_many_arguments)]
    fn choice(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        label: &str,
        comp: &mut Component,
        prop: &str,
        options: &[(&str, &str)],
        default: &str,
    ) {
        kit::text(self.c, x, y + 3., label, true);
        let current = comp.raw(prop).unwrap_or(default).trim().to_string();
        let mut at = options.iter().position(|(_, v)| *v == current).unwrap_or(0);
        let items: Vec<(&str, bool)> = options.iter().map(|(c, _)| (*c, true)).collect();
        let key = self.key(prop);
        if self
            .c
            .combo(&key, x + LABEL, y, w - LABEL, 21., &items, &mut at, true)
        {
            comp.set_raw(prop, options[at].1);
        }
    }

    /// A multi-line list property, one item per line.
    #[allow(clippy::too_many_arguments)]
    fn lines(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        comp: &mut Component,
        prop: &str,
    ) {
        kit::text(self.c, x, y, label, true);
        let mut text = self.buf(prop, || comp.list(prop).unwrap_or_default().join("\n"));
        let key = self.key(prop);
        let response = kit::text_area(
            self.c,
            &key,
            (x, y + 16., w, h),
            &mut text,
            kit::body_font(),
            crate::chrome::INK,
        );
        if response.changed() {
            let lines: Vec<String> = text.lines().map(str::to_string).collect();
            edit::set_lines(comp, prop, &lines);
            self.put(prop, text);
        }
    }
}

const COLORS: [(&str, &str); 9] = [
    ("Black", "clBlack"),
    ("White", "clWhite"),
    ("Red", "clRed"),
    ("Green", "clGreen"),
    ("Blue", "clBlue"),
    ("Navy", "clNavy"),
    ("Maroon", "clMaroon"),
    ("Gray", "clGray"),
    ("Silver", "clSilver"),
];

const ALIGNS: [(&str, &str); 3] = [
    ("Left", "taLeftJustify"),
    ("Right", "taRightJustify"),
    ("Center", "taCenter"),
];

impl App {
    /// Draws the Properties dialog for the editor's draft; returns the
    /// finished draft on OK, `Some(None)` while open, and `None` once
    /// cancelled.
    pub(crate) fn report_properties(&mut self, ctx: &egui::Context, mut p: Props) -> PropsResult {
        if self.reports.take_escape() {
            return PropsResult::Cancelled;
        }
        const W: f32 = 700.;
        const H: f32 = 500.;
        #[derive(Default)]
        struct Clicks {
            ok: bool,
            cancel: bool,
            help: bool,
            tab: Option<usize>,
            browse: bool,
        }
        let targets: Vec<Option<Path>> = (0..TABS.len()).map(|t| p.target(t)).collect();
        let reasons = [
            "",
            "Select a data source first",
            "Select a row first",
            "Select a text cell first",
            "Select a line cell first",
            "Select an image cell first",
        ];
        let tab = p.tab.min(TABS.len() - 1);
        let (k, closed, actions) = kit::window(
            ctx,
            "report_properties",
            "Report Properties",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::STRIP);
                kit::page(c, 102., 5., W - 107., H - 46.);
                let tabs: Vec<kit::VTab> = TABS
                    .iter()
                    .enumerate()
                    .map(|(i, (label, key))| kit::VTab {
                        label,
                        key,
                        unavailable: targets[i].is_none().then_some(reasons[i]),
                    })
                    .collect();
                k.tab = kit::vertical_tabs(c, "props_tab_", 5., 5., 98., &tabs, tab);
                let path_key = format!("{:?}", targets[tab]);
                let mut form = Form {
                    c,
                    bufs: &mut p.bufs,
                    path: path_key,
                };
                match (tab, targets[tab].clone()) {
                    (0, _) => report_tab(&mut form, &mut p.draft.root, &mut p.page, &mut p.error),
                    (1, Some(t)) => {
                        if let Some(comp) = edit::get_mut(&mut p.draft.root, &t) {
                            source_tab(&mut form, comp);
                        }
                    }
                    (2, Some(t)) => {
                        if let Some(comp) = edit::get_mut(&mut p.draft.root, &t) {
                            row_tab(&mut form, comp);
                        }
                    }
                    (3, Some(t)) => {
                        let nearest = nearest_source(&p.draft.root, &t);
                        if let Some(comp) = edit::get_mut(&mut p.draft.root, &t) {
                            cell_tab(&mut form, comp, &nearest);
                        }
                    }
                    (4, Some(t)) => {
                        if let Some(comp) = edit::get_mut(&mut p.draft.root, &t) {
                            line_tab(&mut form, comp);
                        }
                    }
                    (5, Some(t)) => {
                        if let Some(comp) = edit::get_mut(&mut p.draft.root, &t) {
                            k.browse = image_tab(&mut form, comp);
                        }
                    }
                    _ => kit::text(form.c, 120., 24., reasons[tab], true),
                }
                if let Some(e) = &p.error {
                    kit::text(c, 112., H - 70., e, true);
                }
                // The row ends under the page's right edge.
                let row = kit::button_row(
                    c,
                    W - 5.,
                    H - 34.,
                    &[
                        kit::Entry::ok("props_ok"),
                        kit::Entry::cancel("props_cancel"),
                        kit::Entry::help("props_help"),
                    ],
                );
                match row.clicked.as_deref() {
                    Some("props_ok") => k.ok = true,
                    Some("props_cancel") => k.cancel = true,
                    Some("props_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        // The page setup fields write into the payload.
        p.save_page();
        if k.cancel || closed {
            return PropsResult::Cancelled;
        }
        if let Some(t) = k.tab {
            p.tab = t;
        }
        if k.help {
            self.open_help_for("report_properties");
        }
        if k.browse {
            if let (Some(f), Some(t)) = (
                rfd::FileDialog::new()
                    .add_filter("Image", &["png", "gif", "jpg", "jpeg", "bmp"])
                    .pick_file(),
                targets[5].clone(),
            ) {
                if let Some(comp) = edit::get_mut(&mut p.draft.root, &t) {
                    comp.set_string("ImageFileName", &f.display().to_string());
                    p.bufs.retain(|key, _| !key.ends_with(":ImageFileName"));
                }
            }
        }
        if k.ok {
            return PropsResult::Done(Box::new(p.draft));
        }
        PropsResult::Open(Box::new(p))
    }
}

pub enum PropsResult {
    Open(Box<Props>),
    Done(Box<Stream>),
    Cancelled,
}

use crate::App;

/// The table of the nearest data source above a component (for the field list).
fn nearest_source(root: &Component, path: &[usize]) -> String {
    let mut at = path.to_vec();
    loop {
        if let Some(c) = edit::get(root, &at) {
            if edit::kind(c) == Kind::Group {
                return c
                    .string("GroupByTableName")
                    .or_else(|| {
                        c.string("GroupByOuterJoinPath")
                            .or_else(|| c.string("GroupByInnerJoinPath"))
                            .map(|j| j.rsplit(['.', '-']).next().unwrap_or("").to_string())
                    })
                    .unwrap_or_default();
            }
        }
        if at.pop().is_none() {
            return "PROJECT".into();
        }
    }
}

fn report_tab(f: &mut Form, root: &mut Component, page: &mut Page, error: &mut Option<String>) {
    let _ = error;
    f.string(116., 14., 330., "Report title", root, "Title");
    f.string(116., 40., 330., "Report name", root, "ReportName");
    // Page setup, left column.
    kit::section(f.c, 116., 70., 270., 218., "Page Setup");
    let mut at = page::PAPERS
        .iter()
        .position(|p| p.0 == page.paper)
        .unwrap_or(0);
    kit::text(f.c, 126., 92., "Paper", true);
    let papers: Vec<(&str, bool)> = page::PAPERS.iter().map(|p| (p.1, true)).collect();
    if f.c.combo(
        "props_page_paper",
        200.,
        90.,
        176.,
        21.,
        &papers,
        &mut at,
        true,
    ) {
        page.set_paper(page::PAPERS[at].0, page.orientation);
    }
    if kit::radio(
        f.c,
        "props_page_portrait",
        126.,
        118.,
        120.,
        18.,
        "&Portrait",
        page.orientation == Orientation::Portrait,
        true,
    ) {
        page.set_paper(page.paper, Orientation::Portrait);
    }
    if kit::radio(
        f.c,
        "props_page_landscape",
        250.,
        118.,
        120.,
        18.,
        "&Landscape",
        page.orientation == Orientation::Landscape,
        true,
    ) {
        page.set_paper(page.paper, Orientation::Landscape);
    }
    let mut fit = page.fit_to_page;
    if f.c
        .checkbox(
            "props_page_fit",
            126.,
            142.,
            240.,
            18.,
            "Shrink to fit the page width",
            &mut fit,
            true,
        )
        .clicked()
    {
        page.fit_to_page = fit;
    }
    let mut scale = f.buf("page:scale", || format!("{}", page.scale));
    kit::text(f.c, 126., 170., "Scale %", true);
    if f.c
        .edit(
            "props_page_scale",
            200.,
            166.,
            60.,
            21.,
            &mut scale,
            true,
            Align::Min,
        )
        .changed()
    {
        if let Ok(v) = scale.trim().parse::<f32>() {
            page.scale = v.clamp(10., 400.);
        }
        f.put("page:scale", scale);
    }
    let margins: [(&str, f32); 6] = [
        ("Left", page.margins.left),
        ("Right", page.margins.right),
        ("Top", page.margins.top),
        ("Bottom", page.margins.bottom),
        ("Header", page.margins.header),
        ("Footer", page.margins.footer),
    ];
    kit::text(f.c, 126., 198., "Margins (inches)", true);
    for (i, (name, value)) in margins.iter().enumerate() {
        let (cx, cy) = (126. + 128. * (i % 2) as f32, 218. + 24. * (i / 2) as f32);
        kit::text(f.c, cx, cy + 3., name, true);
        let key = format!("page:margin{i}");
        let mut text = f.buf(&key, || format!("{value}"));
        if f.c
            .edit(
                &format!("props_page_margin_{i}"),
                cx + 50.,
                cy,
                60.,
                21.,
                &mut text,
                true,
                Align::Min,
            )
            .changed()
        {
            if let Ok(v) = text.trim().parse::<f32>() {
                match i {
                    0 => page.margins.left = v,
                    1 => page.margins.right = v,
                    2 => page.margins.top = v,
                    3 => page.margins.bottom = v,
                    4 => page.margins.header = v,
                    _ => page.margins.footer = v,
                }
                page.sanitize();
            }
            f.put(&key, text);
        }
    }
    // Header and footer, right column.
    kit::section(f.c, 400., 70., 280., 218., "Header and Footer Text");
    for (band, parts, y0) in [
        ("Header", &mut page.header, 90.),
        ("Footer", &mut page.footer, 190.),
    ] {
        for (j, name) in ["Left", "Middle", "Right"].iter().enumerate() {
            let y = y0 + 28. * j as f32;
            kit::text(
                f.c,
                410.,
                y + 3.,
                &format!("{band} {}", name.to_lowercase()),
                true,
            );
            let key = format!("page:{band}{j}");
            let mut text = f.buf(&key, || parts[j].text.clone());
            if f.c
                .edit(
                    &format!("props_page_{}_{j}", band.to_lowercase()),
                    488.,
                    y,
                    190.,
                    21.,
                    &mut text,
                    true,
                    Align::Min,
                )
                .changed()
            {
                parts[j].text = text.clone();
                f.put(&key, text);
            }
        }
    }
    kit::text(
        f.c,
        116.,
        300.,
        "Header and footer text can hold [date], [time], [page_number] and [total_pages].",
        true,
    );
}

fn source_tab(f: &mut Form, c: &mut Component) {
    // The table: either its own name or the end of the join path.
    kit::text(f.c, 116., 17., "Loop through", true);
    let tables: Vec<(&str, bool)> = data::SOURCES.iter().map(|(_, cap)| (*cap, true)).collect();
    let current = c
        .string("GroupByTableName")
        .or_else(|| {
            c.string("GroupByOuterJoinPath")
                .map(|j| j.rsplit(['.', '-']).next().unwrap_or("").to_string())
        })
        .unwrap_or_default();
    let mut at = data::SOURCES
        .iter()
        .position(|(t, _)| t.eq_ignore_ascii_case(&current))
        .unwrap_or(0);
    let own_table = c.string("GroupByTableName").is_some();
    if f.c.combo(
        "props_source_table",
        266.,
        14.,
        220.,
        21.,
        &tables,
        &mut at,
        own_table,
    ) {
        c.set_string("GroupByTableName", data::SOURCES[at].0);
    }
    f.string(116., 42., 560., "Join path", c, "GroupByOuterJoinPath");
    let tree = c.raw("GroupByType").is_some_and(|t| t.trim() == "gtTree");
    let mut tree_on = tree;
    if f.c
        .checkbox(
            "props_source_tree",
            116.,
            72.,
            300.,
            18.,
            "Organize hierarchically",
            &mut tree_on,
            true,
        )
        .clicked()
    {
        if tree_on {
            c.set_raw("GroupByType", "gtTree");
        } else {
            c.remove("GroupByType");
        }
    }
    f.int(430., 70., 250., "Maximum level", c, "MaxLevels", (0, 1000));
    f.flag(
        116.,
        96.,
        300.,
        "Hide record if no children",
        c,
        "HideRowIfNoChildren",
    );
    f.flag(430., 96., 250., "Hide if empty", c, "HideIfEmpty");
    f.choice(
        116.,
        120.,
        330.,
        "Page break",
        c,
        "PageBreakType",
        &[
            ("None", "pbtNone"),
            ("After each record", "pbtAfterRecord"),
            ("After the set", "pbtAfterSet"),
        ],
        "pbtNone",
    );
    f.lines(
        116.,
        150.,
        270.,
        110.,
        "Filter (one line per condition)",
        c,
        "Filter.Strings",
    );
    f.lines(
        406.,
        150.,
        270.,
        110.,
        "Sort (field ASC or field DESC)",
        c,
        "SortBy.Strings",
    );
    let lines = c.list("Filter.Strings").unwrap_or_default();
    let unread = filter::parse(&lines).unreadable().len();
    if unread > 0 {
        kit::text(
            f.c,
            116.,
            288.,
            &format!("{unread} filter line(s) cannot be read and are ignored."),
            true,
        );
    }
}

fn row_tab(f: &mut Form, c: &mut Component) {
    f.int(116., 14., 340., "Height", c, "Height", (4, 400));
    f.flag(116., 44., 340., "Hide if empty", c, "HideIfEmpty");
    f.lines(
        116.,
        70.,
        340.,
        110.,
        "Row filter (one line per condition)",
        c,
        "Filter.Strings",
    );
    let lines = c.list("Filter.Strings").unwrap_or_default();
    let unread = filter::parse(&lines).unreadable().len();
    if unread > 0 {
        kit::text(
            f.c,
            116.,
            200.,
            &format!("{unread} filter line(s) cannot be read and are ignored."),
            true,
        );
    }
}

fn cell_tab(f: &mut Form, c: &mut Component, source: &str) {
    let kinds = [
        ("Fixed text", "ctCustomText"),
        ("Text with variables", "ctVariable"),
        ("Data field", "ctDataField"),
        ("Field title", "ctFieldTitle"),
        ("Summary of a field", "ctFieldSummary"),
    ];
    let current = c
        .raw("CellType")
        .unwrap_or("ctDataField")
        .trim()
        .to_string();
    f.choice(
        116.,
        14.,
        560.,
        "Cell shows",
        c,
        "CellType",
        &kinds,
        "ctDataField",
    );
    // `ctDataField` is the default and is not written.
    if c.raw("CellType").is_some_and(|t| t.trim() == "ctDataField") {
        c.remove("CellType");
    }
    let shown = c
        .raw("CellType")
        .unwrap_or("ctDataField")
        .trim()
        .to_string();
    let _ = current;
    match shown.as_str() {
        "ctCustomText" => f.string(116., 42., 560., "Text", c, "Text"),
        "ctVariable" => f.string(116., 42., 560., "Text with [variables]", c, "VariableField"),
        _ => {
            f.string(116., 42., 400., "Field", c, "DataField");
            // A pick list of the nearest source's fields.
            let vocab = data::vocabulary(source);
            let items: Vec<(&str, bool)> = std::iter::once(("Pick...", true))
                .chain(vocab.iter().map(|(_, t)| (t.as_str(), true)))
                .collect();
            let mut pick = 0;
            if f.c.combo(
                "props_cell_pick",
                530.,
                42.,
                146.,
                21.,
                &items,
                &mut pick,
                true,
            ) && pick > 0
            {
                let name = vocab[pick - 1].0;
                c.set_string("DataField", name);
                let key = format!("{}:DataField", f.path);
                f.bufs.insert(key, name.to_string());
            }
            if shown == "ctFieldTitle" {
                f.string(
                    116.,
                    70.,
                    560.,
                    "Title (blank for the field's own)",
                    c,
                    "FieldTitle",
                );
            }
            if shown == "ctFieldSummary" {
                f.string(116., 70., 400., "Total over", c, "SummaryGroupByPath");
                let modes: Vec<(&str, &str)> = Aggregate::ALL
                    .iter()
                    .map(|m| (m.caption(), m.text()))
                    .collect();
                f.choice(116., 98., 400., "Function", c, "AggrMode", &modes, "amSum");
            }
        }
    }
    f.int(116., 126., 250., "Left", c, "Left", (0, 5000));
    f.int(386., 126., 290., "Width", c, "Width", (1, 5000));
    f.choice(
        116.,
        154.,
        330.,
        "Text alignment",
        c,
        "Alignment",
        &ALIGNS,
        "taLeftJustify",
    );
    f.number(
        466.,
        154.,
        210.,
        "Text height (px)",
        c,
        "Font.Height",
        (-60, -6),
        true,
    );
    // Style flags.
    let style = c.raw("Font.Style").unwrap_or("[]").to_string();
    let mut flags = [
        style.contains("fsBold"),
        style.contains("fsItalic"),
        style.contains("fsUnderline"),
    ];
    let mut changed = false;
    for (i, label) in ["Bold", "Italic", "Underline"].iter().enumerate() {
        if f.c
            .checkbox(
                &format!("props_cell_style_{i}"),
                116. + 100. * i as f32,
                184.,
                96.,
                18.,
                label,
                &mut flags[i],
                true,
            )
            .clicked()
        {
            changed = true;
        }
    }
    if changed {
        let mut v = vec![];
        for (on, name) in flags.iter().zip(["fsBold", "fsItalic", "fsUnderline"]) {
            if *on {
                v.push(name);
            }
        }
        c.set_raw("Font.Style", &format!("[{}]", v.join(", ")));
        c.set_raw("FontStyle", "rfsCustom");
    }
    let colors: Vec<(&str, &str)> = COLORS.to_vec();
    f.choice(
        116.,
        210.,
        330.,
        "Text color",
        c,
        "Font.Color",
        &colors,
        "clBlack",
    );
    f.choice(
        116.,
        238.,
        330.,
        "Background",
        c,
        "Color",
        &colors,
        "clWhite",
    );
    // Borders.
    let borders = c.raw("Borders").unwrap_or("[]").to_string();
    let mut b = [
        borders.contains("rpLeft"),
        borders.contains("rpTop"),
        borders.contains("rpRight"),
        borders.contains("rpBottom"),
    ];
    kit::text(f.c, 116., 270., "Borders", true);
    let mut border_changed = false;
    for (i, label) in ["Left", "Top", "Right", "Bottom"].iter().enumerate() {
        if f.c
            .checkbox(
                &format!("props_cell_border_{i}"),
                190. + 80. * i as f32,
                268.,
                76.,
                18.,
                label,
                &mut b[i],
                true,
            )
            .clicked()
        {
            border_changed = true;
        }
    }
    if border_changed {
        let names = ["rpLeft", "rpTop", "rpRight", "rpBottom"];
        let on: Vec<&str> = b
            .iter()
            .zip(names)
            .filter(|(x, _)| **x)
            .map(|(_, n)| n)
            .collect();
        if on.is_empty() {
            c.remove("Borders");
        } else {
            c.set_raw("Borders", &format!("[{}]", on.join(", ")));
        }
    }
    f.flag(116., 298., 250., "Hide if empty", c, "HideIfEmpty");
    f.flag(
        386.,
        298.,
        290.,
        "Indent by hierarchy level",
        c,
        "ShowIndent",
    );
    f.flag(
        116.,
        322.,
        560.,
        "Repeat across the report's date range (one cell per period)",
        c,
        "ShowTimeDistrib",
    );
    let _ = Rgb::BLACK;
}

fn line_tab(f: &mut Form, c: &mut Component) {
    f.int(116., 14., 400., "Left", c, "Left", (0, 5000));
    f.int(116., 42., 400., "Width", c, "Width", (1, 5000));
    f.int(116., 70., 400., "Number of lines", c, "NumLines", (1, 20));
    f.choice(
        116.,
        98.,
        400.,
        "Line alignment",
        c,
        "CellAlign",
        &[
            ("Left", "caLeft"),
            ("Center", "caCenter"),
            ("Right", "caRight"),
        ],
        "caCenter",
    );
}

/// The image tab; true when Select image was clicked.
fn image_tab(f: &mut Form, c: &mut Component) -> bool {
    f.string(116., 14., 520., "Image file", c, "ImageFileName");
    let select = kit::button(
        f.c,
        "props_image_select",
        116.,
        42.,
        kit::BUTTON.x,
        kit::BUTTON.y,
        "Select image...",
        true,
    );
    f.int(116., 82., 400., "Left", c, "Left", (0, 5000));
    f.int(116., 110., 400., "Width", c, "Width", (1, 5000));
    f.int(116., 138., 400., "Height", c, "Height", (1, 1000));
    f.choice(
        116.,
        166.,
        400.,
        "Image alignment",
        c,
        "CellAlign",
        &[
            ("Left", "caLeft"),
            ("Center", "caCenter"),
            ("Right", "caRight"),
        ],
        "caCenter",
    );
    select
}

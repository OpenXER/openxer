//! Building and changing a definition: a new blank report, and adding,
//! copying, moving and deleting rows, cells and data sources. Components are
//! addressed by a path of item indexes from the root. Row and section
//! positions (`Top`) are kept in step so the reference reads the layout in
//! the same order.
use super::model::{
    GROUP_BY, IMAGE_CELL, LINE_CELL, RECURSE, REPORT, ROW, ROW_CONTAINER, SECTION, TEXT_CELL,
};
use super::page;
use super::stream::{Component, Item, Prop, Stream};

pub type Path = Vec<usize>;

pub fn get<'a>(root: &'a Component, path: &[usize]) -> Option<&'a Component> {
    let mut at = root;
    for &i in path {
        at = match at.items.get(i)? {
            Item::Child(c) => c,
            _ => return None,
        };
    }
    Some(at)
}

pub fn get_mut<'a>(root: &'a mut Component, path: &[usize]) -> Option<&'a mut Component> {
    let mut at = root;
    for &i in path {
        at = match at.items.get_mut(i)? {
            Item::Child(c) => c,
            _ => return None,
        };
    }
    Some(at)
}

/// What a component is, for the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Report,
    ReportHeader,
    PageHeader,
    PageFooter,
    ReportFooter,
    Detail,
    GroupHeader,
    GroupFooter,
    Group,
    Recurse,
    Row,
    Text,
    Line,
    Image,
    Other,
}

pub fn kind(c: &Component) -> Kind {
    let t = c.raw("ContainerType").unwrap_or("").trim();
    match (c.class.as_str(), t) {
        (REPORT, _) => Kind::Report,
        (ROW_CONTAINER, "rctReportHeader") => Kind::ReportHeader,
        (ROW_CONTAINER, "rctPageHeader") => Kind::PageHeader,
        (ROW_CONTAINER, "rctPageFooter") => Kind::PageFooter,
        (ROW_CONTAINER, "rctReportFooter") => Kind::ReportFooter,
        (ROW_CONTAINER, "rctGroupByHeader") => Kind::GroupHeader,
        (ROW_CONTAINER, "rctGroupbyFooter") => Kind::GroupFooter,
        (SECTION, _) => Kind::Detail,
        (GROUP_BY, _) => Kind::Group,
        (RECURSE, _) => Kind::Recurse,
        (ROW, _) => Kind::Row,
        (TEXT_CELL, _) => Kind::Text,
        (LINE_CELL, _) => Kind::Line,
        (IMAGE_CELL, _) => Kind::Image,
        _ => Kind::Other,
    }
}

/// A caption for a component in the editor's outline.
pub fn caption(c: &Component) -> String {
    match kind(c) {
        Kind::Report => "Report".into(),
        Kind::ReportHeader => "Report header".into(),
        Kind::PageHeader => "Page header".into(),
        Kind::PageFooter => "Page footer".into(),
        Kind::ReportFooter => "Report footer".into(),
        Kind::Detail => "Sections".into(),
        Kind::GroupHeader => "Source header".into(),
        Kind::GroupFooter => "Source footer".into(),
        Kind::Group => {
            let table = c
                .string("GroupByTableName")
                .or_else(|| c.string("GroupByInnerJoinPath"))
                .or_else(|| c.string("GroupByOuterJoinPath"))
                .unwrap_or_default();
            let name = super::data::source_caption(&table);
            let name = if name.is_empty() {
                table.as_str()
            } else {
                name
            };
            if c.raw("GroupByType").is_some_and(|t| t.trim() == "gtTree") {
                format!("Data source: {name} (hierarchy)")
            } else {
                format!("Data source: {name}")
            }
        }
        Kind::Recurse => "Continue with child records".into(),
        Kind::Row => format!("Row, height {}", c.int("Height").unwrap_or(0)),
        Kind::Text => {
            let what = match c.raw("CellType").unwrap_or("").trim() {
                "ctCustomText" => c.string("Text").unwrap_or_default(),
                "ctVariable" => c.string("VariableField").unwrap_or_default(),
                "ctFieldTitle" => format!("Title of {}", c.string("DataField").unwrap_or_default()),
                "ctFieldSummary" => {
                    format!("Total of {}", c.string("DataField").unwrap_or_default())
                }
                _ => c.string("DataField").unwrap_or_default(),
            };
            format!("Cell: {what}")
        }
        Kind::Line => "Line cell".into(),
        Kind::Image => format!(
            "Image cell: {}",
            c.string("ImageFileName").unwrap_or_default()
        ),
        Kind::Other => c.class.clone(),
    }
}

fn prop(c: &mut Component, name: &str, value: &str) {
    c.set_raw(name, value);
}

fn container(indent: usize, ty: &str, align: &str) -> Component {
    let mut c = Component::new(indent, ROW_CONTAINER);
    prop(&mut c, "Top", "1");
    prop(&mut c, "Width", "960");
    prop(&mut c, "Height", "48");
    prop(&mut c, "ContainerAlign", align);
    prop(&mut c, "ContainerType", ty);
    c
}

fn section(indent: usize) -> Component {
    let mut c = Component::new(indent, SECTION);
    prop(&mut c, "Top", "1");
    prop(&mut c, "Width", "960");
    prop(&mut c, "Height", "200");
    prop(&mut c, "ContainerAlign", "alcClient");
    prop(&mut c, "ContainerType", "rctDetail");
    c
}

/// A new report: page settings, the four row containers and one section.
pub fn new_report(title: &str, landscape: bool) -> Stream {
    let mut root = Component::new(0, REPORT);
    prop(&mut root, "Width", if landscape { "1056" } else { "816" });
    prop(&mut root, "Height", "600");
    prop(&mut root, "ParentFont", "False");
    prop(&mut root, "Color", "clWhite");
    root.set_string("Title", title);
    root.set_string("ReportName", title);
    root.set_string("VDBEngineName", "VDBORION");
    let mut payload = page::new_payload();
    let mut settings = page::Page::from_tree(&payload);
    settings.set_paper(
        1,
        if landscape {
            page::Orientation::Landscape
        } else {
            page::Orientation::Portrait
        },
    );
    settings.apply(&mut payload);
    root.set_list(page::PAYLOAD_PROP, &page::payload_lines(&payload));
    for (name, px, bold) in [
        ("FontHeading1", 19, true),
        ("FontHeading2", 16, true),
        ("FontHeading3", 13, true),
        ("FontHeading4", 11, true),
        ("FontBody1", 19, false),
        ("FontBody2", 16, false),
        ("FontBody3", 13, false),
    ] {
        prop(&mut root, &format!("{name}.Color"), "clBlack");
        prop(&mut root, &format!("{name}.Height"), &format!("-{px}"));
        prop(&mut root, &format!("{name}.Name"), "'Arial'");
        prop(
            &mut root,
            &format!("{name}.Style"),
            if bold { "[fsBold]" } else { "[]" },
        );
    }
    let width = if landscape { 960 } else { 720 };
    for (ty, align) in [("rctReportHeader", "alcTop"), ("rctPageHeader", "alcTop")] {
        let mut c = container(2, ty, align);
        c.set_int("Width", width);
        root.items.push(Item::Child(c));
    }
    let mut sec = section(2);
    sec.set_int("Width", width);
    root.items.push(Item::Child(sec));
    for (ty, align) in [
        ("rctPageFooter", "alcBottom"),
        ("rctReportFooter", "alcBottom"),
    ] {
        let mut c = container(2, ty, align);
        c.set_int("Width", width);
        root.items.push(Item::Child(c));
    }
    Stream {
        root,
        eol: "\r\n".into(),
        trailer: "\r\n".into(),
        latin1: false,
    }
}

/// The path of the root-level container of `kind`.
pub fn container_path(s: &Stream, want: Kind) -> Option<Path> {
    s.root
        .items
        .iter()
        .enumerate()
        .find_map(|(i, it)| match it {
            Item::Child(c) if kind(c) == want => Some(vec![i]),
            _ => None,
        })
}

/// Sets `Top` of the rows and data sources in a container so they read in
/// item order, each starting below the one before.
pub fn renumber(c: &mut Component) {
    let mut top = 1;
    for item in &mut c.items {
        let Item::Child(k) = item else { continue };
        if !matches!(kind(k), Kind::Row | Kind::Group | Kind::Recurse) {
            continue;
        }
        k.set_int("Top", top);
        top += k.int("Height").unwrap_or(20).max(1);
    }
}

fn is_row_holder(k: Kind) -> bool {
    matches!(
        k,
        Kind::ReportHeader
            | Kind::PageHeader
            | Kind::PageFooter
            | Kind::ReportFooter
            | Kind::GroupHeader
            | Kind::GroupFooter
            | Kind::Detail
    )
}

/// Adds a row at the end of a row container or section.
pub fn add_row(s: &mut Stream, holder: &[usize], height: i64) -> Result<Path, String> {
    let h = get_mut(&mut s.root, holder).ok_or("There is no such place for a row")?;
    if !is_row_holder(kind(h)) {
        return Err("Rows go in a header, footer or section".into());
    }
    let mut row = Component::new(h.indent + 2, ROW);
    row.set_int("Top", 1);
    prop(
        &mut row,
        "Width",
        &h.int("Width").unwrap_or(960).to_string(),
    );
    row.set_int("Height", height.clamp(4, 400));
    prop(&mut row, "Color", "clWhite");
    prop(&mut row, "ContainerAlign", "alcTop");
    h.items.push(Item::Child(row));
    let index = h.items.len() - 1;
    renumber(h);
    grow_container(h);
    let mut path = holder.to_vec();
    path.push(index);
    Ok(path)
}

/// Makes a row container as tall as its rows.
fn grow_container(h: &mut Component) {
    if matches!(kind(h), Kind::Detail) {
        return;
    }
    let total: i64 = h
        .children()
        .filter(|c| kind(c) == Kind::Row)
        .map(|c| c.int("Height").unwrap_or(20))
        .sum();
    h.set_int("Height", (total + 2).max(48));
}

/// What a new cell shows. The editor adds text, line and image cells; the
/// others are made by the starter library builder and by Properties.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum NewCell {
    Text(String),
    Variable(String),
    Field(String),
    Title {
        field: String,
        title: String,
    },
    Summary {
        field: String,
        path: String,
        mode: String,
    },
    Line,
    Image(String),
}

#[derive(Clone, Debug)]
pub struct CellSpec {
    pub what: NewCell,
    pub left: i64,
    pub width: i64,
    pub right: bool,
    pub bold: bool,
    /// Height of the text in pixels.
    pub px: i64,
    pub border_bottom: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
impl CellSpec {
    pub fn new(what: NewCell, left: i64, width: i64) -> Self {
        Self {
            what,
            left,
            width,
            right: false,
            bold: false,
            px: 13,
            border_bottom: false,
        }
    }
    pub fn heading(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn size(mut self, px: i64) -> Self {
        self.px = px;
        self
    }
    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }
    pub fn underlined(mut self) -> Self {
        self.border_bottom = true;
        self
    }
    fn style(&self) -> &'static str {
        match (self.px, self.bold) {
            (19.., _) => "rfsHeading1",
            (16..=18, _) => "rfsHeading2",
            (_, true) => "rfsHeading3",
            _ => "rfsBody3",
        }
    }
}

/// Adds a cell to a row.
pub fn add_cell(s: &mut Stream, row: &[usize], spec: &CellSpec) -> Result<Path, String> {
    let r = get_mut(&mut s.root, row).ok_or("There is no such row")?;
    if kind(r) != Kind::Row {
        return Err("Cells go in rows".into());
    }
    let height = r.int("Height").unwrap_or(20);
    let class = match spec.what {
        NewCell::Line => LINE_CELL,
        NewCell::Image(_) => IMAGE_CELL,
        _ => TEXT_CELL,
    };
    let mut c = Component::new(r.indent + 2, class);
    if spec.left != 0 {
        c.set_int("Left", spec.left.max(0));
    }
    c.set_int("Width", spec.width.clamp(1, 5000));
    c.set_int("Height", (height - 1).max(1));
    match &spec.what {
        NewCell::Line => {
            prop(&mut c, "Color", "clWhite");
            prop(&mut c, "CellAlign", "caCenter");
        }
        NewCell::Image(file) => {
            prop(&mut c, "Color", "clWhite");
            c.set_string("ImageFileName", file);
        }
        other => {
            prop(&mut c, "ParentFont", "False");
            prop(&mut c, "Color", "clWhite");
            prop(&mut c, "AutoAlignment", "False");
            match other {
                NewCell::Text(t) => {
                    prop(&mut c, "CellType", "ctCustomText");
                    c.set_string("Text", t);
                }
                NewCell::Variable(v) => {
                    prop(&mut c, "CellType", "ctVariable");
                    c.set_string("VariableField", v);
                }
                NewCell::Field(f) => c.set_string("DataField", f),
                NewCell::Title { field, title } => {
                    prop(&mut c, "CellType", "ctFieldTitle");
                    c.set_string("DataField", field);
                    if !title.is_empty() {
                        c.set_string("FieldTitle", title);
                    }
                }
                NewCell::Summary { field, path, mode } => {
                    prop(&mut c, "CellType", "ctFieldSummary");
                    c.set_string("DataField", field);
                    if !path.is_empty() {
                        c.set_string("SummaryGroupByPath", path);
                    }
                    prop(&mut c, "AggrMode", mode);
                }
                _ => {}
            }
            prop(&mut c, "FontStyle", spec.style());
            if spec.border_bottom {
                prop(&mut c, "Borders", "[rpBottom]");
            }
            prop(
                &mut c,
                "Alignment",
                if spec.right {
                    "taRightJustify"
                } else {
                    "taLeftJustify"
                },
            );
            prop(&mut c, "Font.Color", "clBlack");
            prop(&mut c, "Font.Height", &format!("-{}", spec.px.clamp(6, 60)));
            prop(&mut c, "Font.Name", "'Arial'");
            prop(
                &mut c,
                "Font.Style",
                if spec.bold { "[fsBold]" } else { "[]" },
            );
        }
    }
    r.items.push(Item::Child(c));
    let mut path = row.to_vec();
    path.push(r.items.len() - 1);
    Ok(path)
}

/// Adds a data source (a loop over `table`) to a section. A tree source
/// continues with child records where its `Recurse` item sits.
pub fn add_group(
    s: &mut Stream,
    section_path: &[usize],
    table: &str,
    join: Option<&str>,
    tree: bool,
) -> Result<Path, String> {
    let sec = get_mut(&mut s.root, section_path).ok_or("There is no such section")?;
    if kind(sec) != Kind::Detail {
        return Err("Data sources go in a section".into());
    }
    let ind = sec.indent + 2;
    let width = sec.int("Width").unwrap_or(960);
    let mut g = Component::new(ind, GROUP_BY);
    g.set_int("Top", 1);
    g.set_int("Width", width);
    g.set_int("Height", 120);
    prop(&mut g, "ContainerAlign", "alcTop");
    if let Some(j) = join.filter(|j| !j.is_empty()) {
        g.set_string("GroupByOuterJoinPath", j);
    } else {
        g.set_string("GroupByTableName", table);
    }
    if tree {
        prop(&mut g, "GroupByType", "gtTree");
    }
    prop(&mut g, "MaxLevels", "0");
    let mut head = container(ind + 2, "rctGroupByHeader", "alcTop");
    head.set_int("Width", width);
    let mut body = section(ind + 2);
    body.set_int("Width", width);
    if tree {
        let mut rec = Component::new(ind + 4, RECURSE);
        rec.set_int("Top", 1);
        rec.set_int("Width", width);
        rec.set_int("Height", 20);
        prop(&mut rec, "ContainerAlign", "alcTop");
        body.items.push(Item::Child(rec));
    }
    let mut foot = container(ind + 2, "rctGroupbyFooter", "alcBottom");
    foot.set_int("Width", width);
    g.items.push(Item::Child(head));
    g.items.push(Item::Child(body));
    g.items.push(Item::Child(foot));
    sec.items.push(Item::Child(g));
    let index = sec.items.len() - 1;
    renumber(sec);
    let mut path = section_path.to_vec();
    path.push(index);
    Ok(path)
}

/// The header, detail section or footer of a data source.
pub fn group_part(s: &Stream, group: &[usize], want: Kind) -> Option<Path> {
    let g = get(&s.root, group)?;
    g.items.iter().enumerate().find_map(|(i, it)| match it {
        Item::Child(c) if kind(c) == want => {
            let mut p = group.to_vec();
            p.push(i);
            Some(p)
        }
        _ => None,
    })
}

/// Deletes a component (not the report itself or its containers).
pub fn delete(s: &mut Stream, path: &[usize]) -> Result<(), String> {
    let Some((&last, parent)) = path.split_last() else {
        return Err("The report itself cannot be deleted".into());
    };
    let p = get_mut(&mut s.root, parent).ok_or("There is nothing to delete")?;
    match p.items.get(last) {
        Some(Item::Child(c))
            if matches!(
                kind(c),
                Kind::Row | Kind::Group | Kind::Text | Kind::Line | Kind::Image | Kind::Recurse
            ) =>
        {
            p.items.remove(last);
        }
        _ => return Err("Only rows, cells and data sources can be deleted".into()),
    }
    renumber(p);
    grow_container(p);
    Ok(())
}

/// Pastes a copied component at the end of `into` (rows into row holders,
/// cells into rows, data sources into sections), re-indented to fit.
pub fn paste(s: &mut Stream, into: &[usize], copy: &Component) -> Result<Path, String> {
    let holder = get_mut(&mut s.root, into).ok_or("There is no place to paste")?;
    let ok = match kind(copy) {
        Kind::Row => is_row_holder(kind(holder)),
        Kind::Text | Kind::Line | Kind::Image => kind(holder) == Kind::Row,
        Kind::Group => kind(holder) == Kind::Detail,
        _ => false,
    };
    if !ok {
        return Err("That cannot be pasted here".into());
    }
    let mut c = copy.clone();
    c.shift(holder.indent as isize + 2 - c.indent as isize);
    holder.items.push(Item::Child(c));
    renumber(holder);
    grow_container(holder);
    let mut path = into.to_vec();
    path.push(holder.items.len() - 1);
    Ok(path)
}

/// Moves a row, cell or data source one place up (`-1`) or down (`1`)
/// among its siblings of the same kind.
pub fn shift(s: &mut Stream, path: &[usize], by: isize) -> Result<Path, String> {
    let (&last, parent) = path.split_last().ok_or("Nothing to move")?;
    let p = get_mut(&mut s.root, parent).ok_or("Nothing to move")?;
    let here = match p.items.get(last) {
        Some(Item::Child(c)) => kind(c),
        _ => return Err("Nothing to move".into()),
    };
    let mut at = last as isize + by;
    while at >= 0 && (at as usize) < p.items.len() {
        if matches!(&p.items[at as usize], Item::Child(c) if kind(c) == here) {
            p.items.swap(last, at as usize);
            renumber(p);
            let mut out = parent.to_vec();
            out.push(at as usize);
            return Ok(out);
        }
        at += by.signum();
    }
    Err("It is already there".into())
}

/// Every component with its path and depth, in document order, for the
/// editor's outline. Cells are listed under their rows.
pub fn outline(s: &Stream) -> Vec<(Path, usize, String)> {
    let mut out = vec![];
    let mut stack: Vec<(Path, usize)> = vec![(vec![], 0)];
    while let Some((path, depth)) = stack.pop() {
        let Some(c) = get(&s.root, &path) else {
            continue;
        };
        if kind(c) != Kind::Other {
            out.push((path.clone(), depth, caption(c)));
        }
        let kids: Vec<usize> = c
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i, Item::Child(_)))
            .map(|(i, _)| i)
            .collect();
        for i in kids.into_iter().rev() {
            let mut p = path.clone();
            p.push(i);
            stack.push((p, depth + 1));
        }
    }
    out
}

/// A string-list property helper for the Properties dialog.
pub fn set_lines(c: &mut Component, name: &str, lines: &[String]) {
    if lines.is_empty() {
        c.remove(name);
    } else {
        let indent = c
            .items
            .iter()
            .find_map(|i| match i {
                Item::Prop(p) => Some(p.indent),
                _ => None,
            })
            .unwrap_or(c.indent + 2);
        c.set(Prop::from_list(indent, name, lines));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::rpt::model::Report;

    #[test]
    fn a_new_report_reads_back() {
        let s = new_report("Lookahead", true);
        let text = s.write();
        let again = Stream::parse(&text).unwrap();
        assert_eq!(again, s);
        let r = Report::from_stream(&again).unwrap();
        assert_eq!(r.title, "Lookahead");
        assert_eq!(r.page.width, 11.);
        assert_eq!(r.page.footer[1].text, "Page [page_number] of [total_pages]");
    }

    #[test]
    fn rows_cells_and_sources_can_be_added_and_removed() {
        let mut s = new_report("T", true);
        let head = container_path(&s, Kind::PageHeader).unwrap();
        let row = add_row(&mut s, &head, 20).unwrap();
        add_cell(
            &mut s,
            &row,
            &CellSpec::new(NewCell::Text("Activity".into()), 0, 100).heading(),
        )
        .unwrap();
        let detail = container_path(&s, Kind::Detail).unwrap();
        let g = add_group(&mut s, &detail, "TASK", None, false).unwrap();
        let body = group_part(&s, &g, Kind::Detail).unwrap();
        let r2 = add_row(&mut s, &body, 20).unwrap();
        let c = add_cell(
            &mut s,
            &r2,
            &CellSpec::new(NewCell::Field("task_code".into()), 0, 100),
        )
        .unwrap();
        let again = Stream::parse(&s.write()).unwrap();
        assert_eq!(again, s);
        let m = Report::from_stream(&again).unwrap();
        assert_eq!(m.page_header.len(), 1);
        assert_eq!(m.body.len(), 1);
        // Copy, move, delete.
        let source = get(&s.root, &r2).unwrap().clone();
        let copy = paste(&mut s, &body, &source).unwrap();
        assert_eq!(copy.last().copied(), Some(r2.last().unwrap() + 1));
        let moved = shift(&mut s, &copy, -1).unwrap();
        assert_eq!(moved.last().copied(), r2.last().copied());
        assert!(shift(&mut s, &moved, -1).is_err());
        delete(&mut s, &c).unwrap();
        let m = Report::from_stream(&Stream::parse(&s.write()).unwrap()).unwrap();
        let crate::reports::rpt::model::Node::Group(g) = &m.body[0] else {
            panic!()
        };
        assert_eq!(g.body.len(), 2);
        assert!(delete(&mut s, &[]).is_err());
        assert!(delete(&mut s, &head).is_err());
        assert!(!outline(&s).is_empty());
    }

    #[test]
    fn pasting_reindents() {
        let mut s = new_report("T", true);
        let head = container_path(&s, Kind::PageHeader).unwrap();
        let row = add_row(&mut s, &head, 20).unwrap();
        let detail = container_path(&s, Kind::Detail).unwrap();
        let g = add_group(&mut s, &detail, "TASK", None, false).unwrap();
        let body = group_part(&s, &g, Kind::Detail).unwrap();
        let copy = get(&s.root, &row).unwrap().clone();
        let at = paste(&mut s, &body, &copy).unwrap();
        let pasted = get(&s.root, &at).unwrap();
        assert_eq!(pasted.indent, get(&s.root, &body).unwrap().indent + 2);
        assert!(paste(&mut s, &row, &copy).is_err());
        Stream::parse(&s.write()).unwrap();
    }
}

//! Report generation: walks the section loops over the data sources,
//! expands rows and cells for each record, applies filters and sorts and
//! paginates with the page settings. The result is a list of positioned
//! lines that the writers (HTML, text, PDF and print preview) turn into
//! files; nothing in it depends on the clock except what the caller passes
//! in, so the same inputs give the same output.
use super::data::{self, Data, Rec};
use super::filter::{Filter, Op, SortKey};
use super::model::{
    Aggregate, Align, Borders, Cell, Content, Font, Group, Node, PageBreak, Report, Rgb, Row,
};
use super::page::Page;
use crate::grid::columns::Value;
use chrono::{Datelike, NaiveDateTime};
use openxer_engine::calendar::DAY;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Lines a report may produce before it is cut off.
pub const MAX_LINES: usize = 100_000;
/// Records visited and filter tests made before a run is cut off, whether
/// or not they produce lines.
pub const MAX_WORK: u64 = 5_000_000;
/// Periods one repeating cell can expand to.
pub const MAX_PERIODS: usize = 60;
/// Width in pixels of one tree level when a cell indents.
pub const INDENT: f32 = 20.;

#[derive(Clone, Debug)]
pub struct Options {
    pub now: NaiveDateTime,
    pub user: String,
    pub notes: String,
    /// Values for `[name]` placeholders and filter variables.
    pub variables: BTreeMap<String, String>,
    pub max_lines: usize,
    /// Work budget (see [`MAX_WORK`]).
    pub max_work: u64,
}

impl Options {
    pub fn new(now: NaiveDateTime) -> Self {
        Options {
            now,
            user: String::new(),
            notes: String::new(),
            variables: BTreeMap::new(),
            max_lines: MAX_LINES,
            max_work: MAX_WORK,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Text,
    Line(u32),
    Image(String),
}

/// One painted cell, in pixels at 96 per inch from the line's top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Out {
    pub x: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    pub align: Align,
    pub font: Font,
    pub background: Option<Rgb>,
    pub borders: Borders,
    pub kind: Kind,
    /// A data cell (field, summary, title) as opposed to fixed text.
    pub data: bool,
    /// The number or date the text shows, for a spreadsheet cell that sorts
    /// and sums. `None` for text.
    pub typed: Option<Typed>,
}

/// A cell's value as a spreadsheet keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Typed {
    Number(f64),
    /// Days since 1899-12-30.
    Date(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub height: f32,
    pub cells: Vec<Out>,
    pub background: Option<Rgb>,
    pub break_before: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub y: f32,
    pub line: Line,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageOut {
    pub number: usize,
    pub header: Vec<Placed>,
    pub body: Vec<Placed>,
    pub footer: Vec<Placed>,
    /// The header and footer bands (left, middle, right) with placeholders
    /// filled in.
    pub band_header: [String; 3],
    pub band_footer: [String; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub title: String,
    pub settings: Page,
    /// Printing scale (1.0 = 100%) after "fit to page".
    pub scale: f32,
    /// Printable width and height in layout pixels.
    pub width: f32,
    pub height: f32,
    pub pages: Vec<PageOut>,
    pub warnings: Vec<String>,
    /// Records and lines that were left out because of the line limit.
    pub truncated: bool,
}

impl Generated {
    /// Every line of the report once (page header rows and page footer rows
    /// of the first page, then the body of each page).
    pub fn all_lines(&self) -> Vec<&Line> {
        let mut out = vec![];
        if let Some(first) = self.pages.first() {
            out.extend(first.header.iter().map(|p| &p.line));
        }
        for p in &self.pages {
            out.extend(p.body.iter().map(|p| &p.line));
        }
        if let Some(last) = self.pages.last() {
            out.extend(last.footer.iter().map(|p| &p.line));
        }
        out
    }
}

struct Ctx {
    stack: Vec<Rec>,
    levels: Vec<u32>,
    vars: BTreeMap<String, String>,
}

impl Ctx {
    fn rec(&self) -> Rec {
        self.stack.last().copied().unwrap_or(Rec::Project)
    }
    fn level(&self) -> u32 {
        self.levels.last().copied().unwrap_or(0)
    }
}

/// A period of a repeating cell.
struct Period {
    start: i64,
    end: i64,
    label: String,
}

struct Engine<'a, 'b> {
    d: &'b Data<'a>,
    report: &'b Report,
    opts: &'b Options,
    lines: Vec<Line>,
    pending_break: bool,
    truncated: bool,
    work: std::cell::Cell<u64>,
    exhausted: std::cell::Cell<bool>,
    periods: Vec<Period>,
    range_text: String,
    /// Assignment spreads already worked out: (assignment, quantity field).
    spreads: HashMap<(usize, &'static str), Vec<f64>>,
    /// Records of a filtered hierarchy that stay (matches and their parents).
    tree_keep: Option<HashSet<Rec>>,
}

/// Runs a report on a project's data.
pub fn run(report: &Report, d: &Data, opts: &Options) -> Generated {
    let periods = periods_of(report, d);
    let range_text = match (periods.first(), periods.last()) {
        (Some(a), Some(b)) => format!(
            "{} - {}",
            crate::format::date(d.origin(), Some(a.start)),
            crate::format::date(d.origin(), Some(b.end - 1))
        ),
        _ => String::new(),
    };
    let mut e = Engine {
        d,
        report,
        opts,
        lines: vec![],
        pending_break: false,
        truncated: false,
        work: std::cell::Cell::new(0),
        exhausted: std::cell::Cell::new(false),
        periods,
        range_text,
        spreads: HashMap::new(),
        tree_keep: None,
    };
    let mut ctx = Ctx {
        stack: vec![Rec::Project],
        levels: vec![0],
        vars: opts.variables.clone(),
    };
    // Page header and footer rows are worked out once; the body is the
    // report header, the sections and the report footer.
    let page_head = e.collect(|e| e.rows(&report.page_header, &mut ctx));
    let foot = e.collect(|e| e.rows(&report.page_footer, &mut ctx));
    let body = e.collect(|e| {
        e.rows(&report.report_header, &mut ctx);
        e.nodes(&report.body, None, &mut ctx, 0);
        e.rows(&report.report_footer, &mut ctx);
    });
    let truncated = e.truncated || e.exhausted.get();
    let mut warnings = d.take_warnings();
    for u in unreadable_filters(report) {
        warnings.push(format!("A filter line could not be read: {u}"));
    }
    if e.truncated {
        warnings.push(format!(
            "The report was cut off after {} lines",
            opts.max_lines
        ));
    }
    if e.exhausted.get() {
        warnings.push(format!(
            "The report was cut off after {} steps of work (records visited and filters tested)",
            opts.max_work
        ));
    }
    paginate(report, d, opts, page_head, foot, body, warnings, truncated)
}

fn unreadable_filters(report: &Report) -> Vec<String> {
    let mut out = vec![];
    let mut stack: Vec<&Node> = report.body.iter().collect();
    while let Some(n) = stack.pop() {
        if let Node::Group(g) = n {
            out.extend(g.filter.unreadable().iter().map(|s| s.to_string()));
            stack.extend(g.body.iter());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn periods_of(report: &Report, d: &Data) -> Vec<Period> {
    let uses_spread = {
        let mut found = false;
        let mut check = |rows: &[Row]| {
            found |= rows
                .iter()
                .any(|r| r.cells.iter().any(|c| c.spread.is_some()));
        };
        check(&report.report_header);
        check(&report.page_header);
        check(&report.page_footer);
        check(&report.report_footer);
        let mut stack: Vec<&Node> = report.body.iter().collect();
        while let Some(n) = stack.pop() {
            match n {
                Node::Row(r) => check(std::slice::from_ref(r)),
                Node::Group(g) => {
                    check(&g.header);
                    check(&g.footer);
                    stack.extend(g.body.iter());
                }
                Node::Recurse => {}
            }
        }
        found
    };
    if !uses_spread {
        return vec![];
    }
    let start = report
        .start_date
        .as_deref()
        .and_then(|s| d.date_literal(s))
        .or_else(|| d.data_date())
        .unwrap_or(0);
    let Some(first) = d.when(start) else {
        return vec![];
    };
    let finish = report
        .finish_date
        .as_deref()
        .and_then(|s| d.date_literal(s));
    let unit = report.date_units.as_str();
    let mut out = vec![];
    let mut at = first.date();
    let mut at_minute = start;
    while out.len() < MAX_PERIODS {
        let next = match unit {
            "utDay" => at + chrono::Duration::days(1),
            "utMonth" => {
                let (y, m) = (at.year(), at.month());
                let (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
                chrono::NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(at + chrono::Duration::days(28))
            }
            _ => at + chrono::Duration::days(7),
        };
        let len = (next - at).num_days() * DAY;
        let end = at_minute + len;
        out.push(Period {
            start: at_minute,
            end,
            label: crate::format::day(at),
        });
        if finish.is_some_and(|f| end >= f) || (finish.is_none() && out.len() >= 12) {
            break;
        }
        at = next;
        at_minute = end;
    }
    out
}

impl Engine<'_, '_> {
    /// The lines `f` produces, kept apart from the main list.
    fn collect(&mut self, f: impl FnOnce(&mut Self)) -> Vec<Line> {
        let saved = std::mem::take(&mut self.lines);
        f(self);
        std::mem::replace(&mut self.lines, saved)
    }

    /// Charges `n` steps of work; false once the budget is used up.
    fn spend(&self, n: usize) -> bool {
        let used = self.work.get().saturating_add(n as u64);
        self.work.set(used);
        if used > self.opts.max_work {
            self.exhausted.set(true);
            return false;
        }
        true
    }

    /// The run is over: the line limit or the work budget was reached.
    fn stopped(&self) -> bool {
        self.truncated || self.exhausted.get()
    }

    fn push(&mut self, mut line: Line) {
        if self.lines.len() >= self.opts.max_lines {
            self.truncated = true;
            return;
        }
        if std::mem::take(&mut self.pending_break) {
            line.break_before = true;
        }
        self.lines.push(line);
    }

    fn rows(&mut self, rows: &[Row], ctx: &mut Ctx) {
        for r in rows {
            self.row(r, ctx);
        }
    }

    fn nodes(&mut self, nodes: &[Node], tree: Option<(&Group, u32)>, ctx: &mut Ctx, depth: usize) {
        for n in nodes {
            if self.stopped() {
                return;
            }
            match n {
                Node::Row(r) => self.row(r, ctx),
                Node::Group(g) => self.group(g, ctx, depth + 1),
                Node::Recurse => {
                    if let Some((g, level)) = tree {
                        self.recurse(g, level, ctx, depth);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------- groups

    fn group(&mut self, g: &Group, ctx: &mut Ctx, depth: usize) {
        if depth > super::model::MAX_NEST + 2 {
            self.d.warn("Sections are nested too deeply".into());
            return;
        }
        let records = self.group_records(g, ctx);
        if !self.spend(records.len()) {
            return;
        }
        let count = records.len();
        for (n, rec) in records.into_iter().enumerate() {
            if self.stopped() {
                return;
            }
            self.record(g, rec, 1, ctx, depth);
            if g.page_break == PageBreak::AfterRecord && n + 1 < count {
                self.pending_break = true;
            }
        }
        if g.page_break == PageBreak::AfterSet && count > 0 {
            self.pending_break = true;
        }
    }

    fn record(&mut self, g: &Group, rec: Rec, level: u32, ctx: &mut Ctx, depth: usize) {
        if !self.spend(1) {
            return;
        }
        ctx.stack.push(rec);
        ctx.levels.push(if g.tree { level - 1 } else { 0 });
        let mark = self.lines.len();
        self.rows(&g.header, ctx);
        let body_mark = self.lines.len();
        self.nodes(&g.body, g.tree.then_some((g, level)), ctx, depth);
        if g.hide_if_no_children && self.lines.len() == body_mark {
            self.lines.truncate(mark);
        } else {
            self.rows(&g.footer, ctx);
        }
        ctx.stack.pop();
        ctx.levels.pop();
    }

    fn recurse(&mut self, g: &Group, level: u32, ctx: &mut Ctx, depth: usize) {
        if level as usize >= data::MAX_TREE_DEPTH {
            self.d
                .warn("A hierarchy is deeper than the report follows".into());
            return;
        }
        if g.max_levels > 0 && level >= g.max_levels {
            return;
        }
        let parent = ctx.rec();
        let kids = self.d.tree_children(parent);
        let kids = self.filter_sort(g, kids, ctx, true);
        for k in kids {
            if self.stopped() {
                return;
            }
            self.record(g, k, level + 1, ctx, depth);
        }
    }

    /// The records a group loops over for the current context.
    fn group_records(&mut self, g: &Group, ctx: &mut Ctx) -> Vec<Rec> {
        let table = g.table.as_str();
        let joined = g.join.as_deref().filter(|j| !j.is_empty());
        let mut recs: Option<Vec<Rec>> = None;
        if let Some(path) = joined {
            // Try the enclosing records from the innermost outward.
            for &parent in ctx.stack.iter().rev() {
                if let Some(r) = self.d.children(parent, path) {
                    recs = Some(r);
                    break;
                }
            }
            if recs.is_none() {
                self.d
                    .warn(format!("No relation to {path} from the enclosing section"));
            }
        }
        let recs = recs.or_else(|| self.d.records(table)).unwrap_or_else(|| {
            self.d.warn(format!("Unknown data source {table}"));
            vec![]
        });
        if g.tree {
            // Start at the top of the hierarchy.
            let all: HashSet<Rec> = recs.iter().copied().collect();
            let roots: Vec<Rec> = recs
                .iter()
                .copied()
                .filter(|r| {
                    self.d.is_root(*r) || self.d.parent_of(*r).is_none_or(|p| !all.contains(&p))
                })
                .collect();
            return self.filter_sort_tree(g, roots, recs, ctx);
        }
        self.filter_sort(g, recs, ctx, false)
    }

    /// Roots of a filtered tree: a node stays when it matches or when a
    /// node below it does.
    fn filter_sort_tree(
        &mut self,
        g: &Group,
        roots: Vec<Rec>,
        all: Vec<Rec>,
        ctx: &mut Ctx,
    ) -> Vec<Rec> {
        if !g.filter.is_empty() {
            let mut keep: HashSet<Rec> = HashSet::new();
            for r in &all {
                if self.matches(&g.filter, *r, ctx) {
                    let mut at = Some(*r);
                    let mut guard = 0;
                    while let Some(a) = at {
                        if !keep.insert(a) || guard > data::MAX_TREE_DEPTH * 4 {
                            break;
                        }
                        guard += 1;
                        at = self.d.parent_of(a);
                    }
                }
            }
            let roots: Vec<Rec> = roots.into_iter().filter(|r| keep.contains(r)).collect();
            self.tree_keep = Some(keep);
            return self.sorted(g, roots, ctx);
        }
        self.tree_keep = None;
        self.sorted(g, roots, ctx)
    }

    fn filter_sort(
        &mut self,
        g: &Group,
        recs: Vec<Rec>,
        ctx: &mut Ctx,
        tree_child: bool,
    ) -> Vec<Rec> {
        let recs: Vec<Rec> = if g.filter.is_empty() {
            recs
        } else if tree_child {
            match &self.tree_keep {
                Some(keep) => recs.into_iter().filter(|r| keep.contains(r)).collect(),
                None => recs,
            }
        } else {
            recs.into_iter()
                .filter(|r| self.matches(&g.filter, *r, ctx))
                .collect()
        };
        self.sorted(g, recs, ctx)
    }

    fn sorted(&mut self, g: &Group, recs: Vec<Rec>, _ctx: &mut Ctx) -> Vec<Rec> {
        if g.sort.is_empty() || recs.len() < 2 {
            return recs;
        }
        let keyed: Vec<(Rec, Vec<Value>)> = recs
            .into_iter()
            .map(|r| {
                let keys = g.sort.iter().map(|k| self.d.value(r, &k.field)).collect();
                (r, keys)
            })
            .collect();
        let mut keyed = keyed;
        keyed.sort_by(|a, b| compare_keys(&g.sort, &a.1, &b.1));
        keyed.into_iter().map(|(r, _)| r).collect()
    }

    // ------------------------------------------------------------ filters

    fn matches(&self, f: &Filter, rec: Rec, ctx: &Ctx) -> bool {
        if !self.spend(1) {
            return false;
        }
        match f {
            Filter::And(v) => v.iter().all(|x| self.matches(x, rec, ctx)),
            Filter::Or(v) => v.is_empty() || v.iter().any(|x| self.matches(x, rec, ctx)),
            Filter::Unreadable(_) => true,
            Filter::Cmp { field, op, values } => self.compare(field, *op, values, rec, ctx),
        }
    }

    /// `[name]` as a value from the run variables or an enclosing record.
    fn literal(&self, s: &str, rec: Rec, ctx: &Ctx) -> Option<String> {
        let Some(name) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) else {
            return Some(s.to_string());
        };
        if let Some(v) = ctx.vars.get(name) {
            return Some(v.clone());
        }
        for &r in ctx.stack.iter().rev() {
            if r == rec {
                continue;
            }
            if let Some(t) = self.d.probe(r, name) {
                return Some(t);
            }
        }
        self.d
            .warn(format!("Filter variable [{name}] has no value"));
        None
    }

    fn compare(&self, field: &str, op: Op, values: &[String], rec: Rec, ctx: &Ctx) -> bool {
        let mut lits = vec![];
        for v in values {
            match self.literal(v, rec, ctx) {
                Some(s) => lits.push(s),
                None => return true,
            }
        }
        let v = self.d.value(rec, field);
        let first = lits.first().map(String::as_str).unwrap_or("");
        let ord = match &v {
            Value::Date { minute, .. } => {
                let Some(a) = self.d.date_literal(first) else {
                    self.d.warn(format!("{first} is not a date"));
                    return true;
                };
                let day = |s: &str| !s.contains(':');
                if op == Op::Between {
                    let b = lits.get(1).and_then(|s| self.d.date_literal(s));
                    let Some(b) = b else { return true };
                    let end = if day(&lits[1]) { b + DAY - 1 } else { b };
                    return *minute >= a && *minute <= end;
                }
                let upper = if day(first) { a + DAY - 1 } else { a };
                return match op {
                    Op::Eq => *minute >= a && *minute <= upper,
                    Op::Ne => *minute < a || *minute > upper,
                    Op::Gt => *minute > upper,
                    Op::Ge => *minute >= a,
                    Op::Lt => *minute < a,
                    Op::Le => *minute <= upper,
                    _ => true,
                };
            }
            Value::Empty => {
                // A blank is only equal to a blank.
                return match op {
                    Op::Eq => first.is_empty(),
                    Op::Ne => !first.is_empty(),
                    _ => false,
                };
            }
            other => match (Data::number(other), first.trim().parse::<f64>()) {
                (Some(x), Ok(y)) => {
                    if op == Op::Between {
                        let Some(z) = lits.get(1).and_then(|s| s.trim().parse::<f64>().ok()) else {
                            return true;
                        };
                        return x >= y && x <= z;
                    }
                    x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                }
                _ => {
                    let shown = self.d.show(rec, other);
                    let (a, b) = (shown.to_lowercase(), first.to_lowercase());
                    if op == Op::Like {
                        return like(&a, &b);
                    }
                    let flag = |s: &str| match s {
                        "y" | "yes" | "true" => Some(true),
                        "n" | "no" | "false" => Some(false),
                        _ => None,
                    };
                    let (a, b) = match (flag(&a), flag(&b)) {
                        (Some(x), Some(y)) => (x.to_string(), y.to_string()),
                        _ => (a, b),
                    };
                    a.cmp(&b)
                }
            },
        };
        match op {
            Op::Eq => ord.is_eq(),
            Op::Ne => ord.is_ne(),
            Op::Gt => ord.is_gt(),
            Op::Ge => ord.is_ge(),
            Op::Lt => ord.is_lt(),
            Op::Le => ord.is_le(),
            Op::Like => like(
                &self.d.text(rec, field).to_lowercase(),
                &first.to_lowercase(),
            ),
            Op::Between => true,
        }
    }

    // --------------------------------------------------------------- rows

    fn row(&mut self, row: &Row, ctx: &mut Ctx) {
        if self.stopped() {
            return;
        }
        if !row.filter.is_empty() && !self.matches(&row.filter, ctx.rec(), ctx) {
            return;
        }
        let mut cells = vec![];
        for c in &row.cells {
            self.cell(c, ctx, &mut cells);
        }
        if row.hide_if_empty && !cells.iter().any(|c| c.data && !c.text.trim().is_empty()) {
            return;
        }
        self.push(Line {
            height: row.height,
            cells,
            background: row.background,
            break_before: false,
        });
    }

    fn cell(&mut self, c: &Cell, ctx: &mut Ctx, out: &mut Vec<Out>) {
        let rec = ctx.rec();
        let indent = if c.show_indent {
            INDENT * ctx.level() as f32
        } else {
            0.
        };
        let make = |text: String,
                    x: f32,
                    right_default: bool,
                    kind: Kind,
                    data: bool,
                    typed: Option<Typed>| {
            let align = c.align.unwrap_or(if right_default {
                Align::Right
            } else {
                Align::Left
            });
            Out {
                x,
                w: (c.width - indent).max(0.),
                h: c.height,
                text,
                align,
                font: c.font.clone(),
                background: c.background,
                borders: c.borders,
                kind,
                data,
                typed,
            }
        };
        match &c.content {
            Content::Line { count } => out.push(make(
                String::new(),
                c.left + indent,
                false,
                Kind::Line(*count),
                false,
                None,
            )),
            Content::Image { file } => out.push(make(
                String::new(),
                c.left + indent,
                false,
                Kind::Image(file.clone()),
                false,
                None,
            )),
            content => {
                let spread = c.spread.is_some() && !self.periods.is_empty();
                let count = if spread { self.periods.len() } else { 1 };
                let series = if spread {
                    self.series(content, ctx)
                } else {
                    vec![]
                };
                for k in 0..count {
                    let (text, right, data, typed) = if spread {
                        let (text, right, data) = self.spread_text(content, k, &series, ctx);
                        (text, right, data, None)
                    } else {
                        self.cell_text(content, rec, ctx)
                    };
                    if c.hide_if_empty && text.trim().is_empty() {
                        continue;
                    }
                    let x = c.left + indent + k as f32 * c.width;
                    out.push(make(clean(&text), x, right, Kind::Text, data, typed));
                }
            }
        }
    }

    /// Text, whether it is a number (right aligned), whether it is data,
    /// and the number or date it shows.
    fn cell_text(
        &self,
        content: &Content,
        rec: Rec,
        ctx: &Ctx,
    ) -> (String, bool, bool, Option<Typed>) {
        match content {
            Content::Text(t) => (t.clone(), false, false, None),
            Content::Variable(v) => (self.substitute(v, rec, ctx, None), false, false, None),
            Content::Field(f) => {
                let v = self.d.value(rec, f);
                let text = self.d.text(rec, f);
                let typed = typed(&v, &text, self.d.p.model.origin_serial);
                (text, is_number(&v), true, typed)
            }
            Content::Title { field, title } => (
                if title.is_empty() {
                    data::title(field)
                } else {
                    title.clone()
                },
                false,
                false,
                None,
            ),
            Content::Summary { field, path, mode } => {
                let scope = self.summary_scope(field, path, ctx);
                let v = self.aggregate(&scope, field, *mode);
                let at = scope.first().copied().unwrap_or(rec);
                let text = self.d.show(at, &v);
                let typed = typed(&v, &text, self.d.p.model.origin_serial);
                (text, is_number(&v), true, typed)
            }
            Content::Line { .. } | Content::Image { .. } => (String::new(), false, false, None),
        }
    }

    /// `[name]` placeholders: report variables, then fields of the current
    /// record chain; unknown ones stay as written (page numbers are filled
    /// in when the report is paginated).
    fn substitute(&self, text: &str, rec: Rec, ctx: &Ctx, period: Option<usize>) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find('[') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let Some(close) = after.find(']') else {
                out.push_str(&rest[open..]);
                rest = "";
                break;
            };
            let name = &after[..close];
            let value = match name {
                "title" => Some(self.report.title.clone()),
                "notes" => Some(self.opts.notes.clone()),
                "user" | "user_name" => Some(self.opts.user.clone()),
                "page_number" | "total_pages" => None,
                "column_date" => period
                    .and_then(|k| self.periods.get(k))
                    .map(|p| p.label.clone()),
                "date_range" => Some(self.range_text.clone()),
                _ if name.starts_with("rpt_footer") => Some(String::new()),
                _ => ctx
                    .vars
                    .get(name)
                    .cloned()
                    .or_else(|| self.d.variable(name))
                    .or_else(|| {
                        ctx.stack
                            .iter()
                            .rev()
                            .chain(std::iter::once(&rec))
                            .find_map(|r| self.d.probe(*r, name))
                    }),
            };
            match value {
                Some(v) => out.push_str(&v),
                None => {
                    out.push('[');
                    out.push_str(name);
                    out.push(']');
                }
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }

    // ---------------------------------------------------------- summaries

    /// The records a summary adds up: the nearest enclosing record that
    /// contains the field's source.
    fn summary_scope(&self, field: &str, path: &str, ctx: &Ctx) -> Vec<Rec> {
        let from_field = field
            .split_once('|')
            .map(|(t, _)| t.split('.').next_back().unwrap_or(t).to_ascii_uppercase());
        let from_path = {
            let p = path.trim();
            if let Some((head, _)) = p.split_once('-') {
                Some(head.to_ascii_uppercase())
            } else {
                p.split(':')
                    .rfind(|s| !s.is_empty())
                    .map(|s| s.to_ascii_uppercase())
            }
        }
        .filter(|s| !s.is_empty());
        let target = from_field
            .filter(|t| self.d.records(t).is_some())
            .or(from_path.filter(|t| self.d.records(t).is_some()))
            .unwrap_or_else(|| ctx.rec().table().to_string());
        for &r in ctx.stack.iter().rev() {
            if let Some(v) = self.d.below(r, &target) {
                return v;
            }
        }
        vec![]
    }

    fn aggregate(&self, scope: &[Rec], field: &str, mode: Aggregate) -> Value {
        if !self.spend(scope.len()) {
            return Value::Empty;
        }
        if mode == Aggregate::Count {
            return Value::Number(scope.len() as f64);
        }
        let values: Vec<Value> = scope.iter().map(|r| self.d.value(*r, field)).collect();
        let kind = values.iter().find(|v| !matches!(v, Value::Empty));
        let Some(kind) = kind else {
            return Value::Empty;
        };
        match mode {
            Aggregate::Min | Aggregate::Max => {
                let pick = values
                    .iter()
                    .filter(|v| !matches!(v, Value::Empty))
                    .cloned();
                let best = if mode == Aggregate::Min {
                    pick.min_by(Data::compare)
                } else {
                    pick.max_by(Data::compare)
                };
                best.unwrap_or(Value::Empty)
            }
            _ => {
                let nums: Vec<f64> = values.iter().filter_map(|v| v.number()).collect();
                if nums.is_empty() {
                    return Value::Empty;
                }
                let sum: f64 = nums.iter().sum();
                let x = if mode == Aggregate::Average {
                    sum / nums.len() as f64
                } else {
                    sum
                };
                match kind {
                    Value::Duration(_) => Value::Duration(x.round() as i64),
                    Value::Cost(_) => Value::Cost(x),
                    Value::Units(_) => Value::Units(x),
                    Value::Percent(_) => Value::Percent(x),
                    _ => Value::Number(x),
                }
            }
        }
    }

    // ----------------------------------------------------- repeating cells

    /// Per-period quantities of a repeating cell's field over the scope.
    fn series(&mut self, content: &Content, ctx: &Ctx) -> Vec<f64> {
        let (field, path) = match content {
            Content::Field(f) => (f.as_str(), ""),
            Content::Summary { field, path, .. } => (field.as_str(), path.as_str()),
            _ => return vec![],
        };
        let name = field
            .rsplit('|')
            .next()
            .unwrap_or(field)
            .to_ascii_lowercase();
        let Some(key) = [
            "remain_qty",
            "remain_cost",
            "target_qty",
            "target_cost",
            "act_qty",
            "act_cost",
        ]
        .into_iter()
        .find(|k| *k == name) else {
            return vec![];
        };
        let scope = if matches!(content, Content::Field(_)) {
            match ctx.rec() {
                r @ (Rec::Assign(_) | Rec::Task(_) | Rec::Wbs(_) | Rec::Rsrc(_)) => {
                    self.d.below(r, "TASKRSRC").unwrap_or_default()
                }
                _ => vec![],
            }
        } else {
            self.summary_scope(field, path, ctx)
        };
        let ranges: Vec<(i64, i64)> = self.periods.iter().map(|p| (p.start, p.end)).collect();
        let mut total = vec![0.; ranges.len()];
        for r in scope {
            let Rec::Assign(a) = r else { continue };
            let spread = self.spread_of(a, key, &ranges);
            for (t, s) in total.iter_mut().zip(spread) {
                *t += s;
            }
        }
        total
    }

    fn spread_of(&mut self, a: usize, key: &'static str, ranges: &[(i64, i64)]) -> Vec<f64> {
        if let Some(v) = self.spreads.get(&(a, key)) {
            return v.clone();
        }
        let asn = &self.d.assigns[a];
        let costs = key.ends_with("cost");
        let base = if costs { asn.as_costs() } else { asn.clone() };
        let v = match key {
            "remain_qty" | "remain_cost" => {
                crate::views::assignments::remaining_spread(self.d.p, &base, ranges)
            }
            "act_qty" | "act_cost" => {
                crate::views::assignments::actual_spread(self.d.p, &base, ranges)
            }
            // Budget: the same spread as the usage views (planned dates,
            // the assignment's own window, its curve).
            _ => crate::views::assignments::budget_spread(self.d.p, &base, ranges),
        };
        self.spreads.insert((a, key), v.clone());
        v
    }

    fn spread_text(
        &self,
        content: &Content,
        k: usize,
        series: &[f64],
        ctx: &Ctx,
    ) -> (String, bool, bool) {
        let rec = ctx.rec();
        match content {
            Content::Variable(v) => (self.substitute(v, rec, ctx, Some(k)), false, false),
            Content::Title { .. } | Content::Text(_) => {
                let (t, r, d, _) = self.cell_text(content, rec, ctx);
                (t, r, d)
            }
            Content::Field(f) | Content::Summary { field: f, .. } => match series.get(k).copied() {
                Some(x) if x.abs() > 1e-9 => {
                    let name = f.rsplit('|').next().unwrap_or(f);
                    let text = if name.ends_with("cost") {
                        crate::format::cost(x)
                    } else {
                        crate::format::units(x)
                    };
                    (text, true, true)
                }
                _ => (String::new(), true, true),
            },
            _ => (String::new(), false, false),
        }
    }
}

fn clean(s: &str) -> String {
    s.replace(['\r', '\n', '\t'], " ")
}

/// The spreadsheet value of a cell showing `text` for `v`: dates as date
/// serials, money, counts and percents as their numbers, and durations and
/// units as the number their text shows (in the user's display unit).
/// Blank text stays blank.
fn typed(v: &Value, text: &str, origin: i64) -> Option<Typed> {
    if text.trim().is_empty() {
        return None;
    }
    match v {
        Value::Date { minute, .. } => {
            Some(Typed::Date(origin as f64 + *minute as f64 / DAY as f64))
        }
        Value::Cost(x) | Value::Number(x) | Value::Percent(x) => Some(Typed::Number(*x)),
        Value::Duration(_) | Value::Units(_) => {
            let t = text
                .trim()
                .trim_end_matches(|c: char| c.is_alphabetic() || c == '/' || c == ' ');
            t.parse::<f64>()
                .ok()
                .or_else(|| {
                    t.contains('.')
                        .then(|| t.replace(',', "").parse::<f64>().ok())
                        .flatten()
                })
                .filter(|x| x.is_finite())
                .map(Typed::Number)
        }
        _ => None,
    }
}

fn is_number(v: &Value) -> bool {
    !matches!(
        v,
        Value::Text(_) | Value::Empty | Value::Flag(_) | Value::Date { .. }
    )
}

/// `like` with `%` for any run of characters.
fn like(value: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('%').collect();
    if parts.len() == 1 {
        return value == pattern;
    }
    let mut at = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match value[at..].find(part) {
            Some(found) => {
                if i == 0 && found != 0 {
                    return false;
                }
                at += found + part.len();
            }
            None => return false,
        }
    }
    parts.last().is_some_and(|l| l.is_empty()) || value.ends_with(parts[parts.len() - 1])
}

fn compare_keys(keys: &[SortKey], a: &[Value], b: &[Value]) -> std::cmp::Ordering {
    for (k, (x, y)) in keys.iter().zip(a.iter().zip(b)) {
        let o = Data::compare(x, y);
        let o = if k.descending { o.reverse() } else { o };
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    std::cmp::Ordering::Equal
}

// ----------------------------------------------------------- pagination

#[allow(clippy::too_many_arguments)]
fn paginate(
    report: &Report,
    d: &Data,
    opts: &Options,
    page_header: Vec<Line>,
    page_footer: Vec<Line>,
    body: Vec<Line>,
    warnings: Vec<String>,
    truncated: bool,
) -> Generated {
    let settings = report.page.clone();
    let (cw, ch) = settings.content();
    let mut scale = settings.scale / 100.;
    let needed = body
        .iter()
        .chain(&page_header)
        .chain(&page_footer)
        .flat_map(|l| l.cells.iter())
        .map(|c| c.x + c.w)
        .fold(0., f32::max);
    let mut width = cw * 96. / scale;
    if settings.fit_to_page && needed > width {
        scale *= width / needed;
        width = cw * 96. / scale;
    }
    let height = ch * 96. / scale;
    let head_h: f32 = page_header.iter().map(|l| l.height).sum();
    let foot_h: f32 = page_footer.iter().map(|l| l.height).sum();
    let room = (height - head_h - foot_h).max(40.);
    // Split the body into pages.
    let mut pages: Vec<Vec<Placed>> = vec![vec![]];
    let mut y = 0.;
    for line in body {
        let full = y + line.height > room && !pages.last().is_some_and(Vec::is_empty);
        if full || (line.break_before && !pages.last().is_some_and(Vec::is_empty)) {
            pages.push(vec![]);
            y = 0.;
        }
        let h = line.height;
        pages.last_mut().expect("a page").push(Placed { y, line });
        y += h;
    }
    let total = pages.len();
    let fill = |s: &str, n: usize| -> String {
        let mut text = s.to_string();
        for (k, v) in [
            ("[page_number]", n.to_string()),
            ("[total_pages]", total.to_string()),
            ("[date]", crate::format::day(opts.now.date())),
            ("[time]", opts.now.format("%H:%M").to_string()),
            ("[title]", report.title.clone()),
            ("[user]", opts.user.clone()),
        ] {
            text = text.replace(k, &v);
        }
        for name in [
            "project_name",
            "project_code",
            "data_date",
            "project_start_date",
            "project_end_date",
        ] {
            let k = format!("[{name}]");
            if text.contains(&k) {
                text = text.replace(&k, &d.variable(name).unwrap_or_default());
            }
        }
        text
    };
    let band = |parts: &[super::page::Part; 3], n: usize| -> [String; 3] {
        [
            fill(&parts[0].text, n),
            fill(&parts[1].text, n),
            fill(&parts[2].text, n),
        ]
    };
    let place = |lines: &[Line], n: usize| -> Vec<Placed> {
        let mut y = 0.;
        lines
            .iter()
            .map(|l| {
                let mut line = l.clone();
                for c in &mut line.cells {
                    if c.text.contains("[page_number]") || c.text.contains("[total_pages]") {
                        c.text = fill(&c.text, n);
                    }
                }
                let p = Placed { y, line };
                y += l.height;
                p
            })
            .collect()
    };
    let out_pages = pages
        .into_iter()
        .enumerate()
        .map(|(i, mut body)| {
            let n = i + 1;
            for p in &mut body {
                for c in &mut p.line.cells {
                    if c.text.contains("[page_number]") || c.text.contains("[total_pages]") {
                        c.text = fill(&c.text, n);
                    }
                }
            }
            PageOut {
                number: n,
                header: place(&page_header, n),
                body,
                footer: place(&page_footer, n),
                band_header: band(&settings.header, n),
                band_footer: band(&settings.footer, n),
            }
        })
        .collect();
    Generated {
        title: report.title.clone(),
        settings,
        scale,
        width,
        height,
        pages: out_pages,
        warnings,
        truncated,
    }
}

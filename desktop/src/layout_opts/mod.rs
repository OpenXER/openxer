//! Layout display options of the Activities view: Line Numbers, Progress
//! Spotlight, Collapse To, Table Font and Row, Chart Font and Colors, New
//! Layout, Pause Auto-Sort, Cell Tips and the View > Check
//! Schedule filters. The table and chart options live in the working
//! layout (`grid::layout::TableDisplay`) and the Gantt settings
//! (`ChartOptions`), so they are saved with named layouts. None of these
//! commands edits the project, so none takes an undo step.
use super::*;
use crate::grid::layout::{self, TableFont};
use crate::grid::Row;
pub mod checks;
mod dialogs;
#[cfg(test)]
mod tests;

pub(crate) use dialogs::{sample_button, CollapseTarget, Drafts, TableTarget};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// What the optimized row heights were measured for.
#[derive(Clone, PartialEq)]
pub(crate) struct HeightKey {
    serial: u64,
    columns: Vec<layout::ColumnSetting>,
    font: TableFont,
    lines: u8,
    formats: format::Formats,
}

/// Row heights by activity and what they were measured for.
pub(crate) type MeasuredHeights = (HeightKey, Rc<HashMap<usize, f32>>);

/// A row order kept while auto-reorganization is off.
pub(crate) struct Frozen {
    /// Inputs that reorganize the table when the user changes them.
    key: FreezeKey,
    rows: Vec<FrozenRow>,
    bands: Vec<grid::group::Band>,
    /// Members of each band by `task_id`.
    members: Vec<Vec<i64>>,
}

#[derive(Clone, Copy)]
enum FrozenRow {
    Group(usize),
    Activity(i64),
}

#[derive(Clone, PartialEq)]
struct FreezeKey {
    project: i64,
    multi: bool,
    filter: grid::filters::FilterSelection,
    group_sort: layout::GroupSort,
    users: Vec<grid::filters::UserFilter>,
    query: String,
    wbs_filter: Option<i64>,
    collapsed: BTreeSet<String>,
}

#[derive(Default)]
pub struct LayoutOpts {
    /// Tools > Pause Auto-Sort.
    pub reorganize_off: bool,
    /// The spotlight's end after the data date line was dragged; `None`
    /// is one timescale interval after the data date.
    pub spotlight_end: Option<i64>,
    /// Cell Tips: the whole text of the truncated cell under the pointer.
    pub hint: Option<String>,
    pub(crate) frozen: RefCell<Option<Frozen>>,
    /// The table's bands are the frozen ones, not those of the last build.
    pub(crate) bands_frozen: Cell<bool>,
    /// Heights of rows sized by their content, by activity.
    pub(crate) heights: RefCell<Option<MeasuredHeights>>,
    pub(crate) drafts: Drafts,
    /// Whose bands the open Collapse To dialog collapses.
    pub(crate) collapse_for: CollapseTarget,
}

/// Minutes from the model origin after adding `months` calendar months to
/// `minute`.
fn add_months(origin_serial: i64, minute: i64, months: u32) -> i64 {
    let origin = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| d + chrono::Duration::days(origin_serial));
    origin
        .and_then(|o| {
            let at = o + chrono::Duration::minutes(minute);
            at.checked_add_months(chrono::Months::new(months))
                .map(|later| (later - o).num_minutes())
        })
        .unwrap_or(minute + i64::from(months) * 30 * DAY)
}

/// Room the Show Icons glyph takes before the first column's text.
pub(crate) const GLYPH_WIDTH: f32 = 15.;

/// Show Icons: a small glyph for the activity's type, drawn for OpenXER
/// (not a reference image), at the left of `cell` and centered on it.
/// Tasks are a bar filled by status, milestones a diamond, level of
/// effort a thin bar with end ticks and WBS summaries a bar with caps.
pub(crate) fn type_glyph(p: &egui::Painter, cell: Rect, a: &Activity, selected: bool) {
    let ink = if selected { Color32::WHITE } else { INK };
    let x = cell.left().round() + 1.;
    let y = cell.top().round() + ((cell.height() - 1. - 7.) / 2.).floor().max(0.);
    let rect = |x0: f32, y0: f32, w: f32, h: f32| {
        Rect::from_min_size(Pos2::new(x + x0, y + y0), Vec2::new(w, h))
    };
    // Not-started work in the remaining-work blue, finished work in the
    // actual-work teal.
    let green = crate::theme::color(crate::theme::BLUE);
    let blue = crate::theme::color(crate::theme::TEAL);
    match a.task_type {
        TaskType::Mile | TaskType::FinMile => {
            let c = Pos2::new(x + 5.5, y + 3.5);
            let fill = if a.is_completed() { blue } else { ink };
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + Vec2::new(0., -4.),
                    c + Vec2::new(4., 0.),
                    c + Vec2::new(0., 4.),
                    c + Vec2::new(-4., 0.),
                ],
                fill,
                Stroke::NONE,
            ));
        }
        TaskType::Wbs => {
            chrome::fill(p, rect(0., 1., 11., 3.), ink);
            chrome::fill(p, rect(0., 4., 2., 2.), ink);
            chrome::fill(p, rect(9., 4., 2., 2.), ink);
        }
        TaskType::Loe => {
            chrome::fill(p, rect(0., 2., 11., 2.), ink);
            chrome::fill(p, rect(0., 0., 1., 6.), ink);
            chrome::fill(p, rect(10., 0., 1., 6.), ink);
        }
        _ => {
            chrome::fill(p, rect(0., 0., 11., 6.), ink);
            let inner = rect(1., 1., 9., 4.);
            let fill = match a.status {
                StatusType::Completed => blue,
                _ => green,
            };
            chrome::fill(p, inner, fill);
            if a.status == StatusType::InProgress {
                chrome::fill(p, rect(1., 1., 4., 4.), blue);
            }
        }
    }
}

/// A galley of `text` in the table font; italics slant the glyphs. `wrap`
/// gives a width and the most lines (0 for no limit).
pub(crate) fn table_galley(
    painter: &egui::Painter,
    text: String,
    font: &TableFont,
    color: Color32,
    wrap: Option<(f32, u8)>,
) -> std::sync::Arc<egui::Galley> {
    if font.is_default() && wrap.is_none() {
        return painter.layout_no_wrap(text, FontId::proportional(FONT_SIZE), color);
    }
    painter.fonts_mut(|f| f.layout_job(table_job(text, font, color, wrap)))
}

/// `table_galley` cut to one line `width` wide, ending in an ellipsis
/// when the text does not fit (a clipped cell shows that it is clipped).
pub(crate) fn table_galley_elided(
    painter: &egui::Painter,
    text: String,
    font: &TableFont,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = table_job(text, font, color, Some((width, 1)));
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    painter.fonts_mut(|f| f.layout_job(job))
}

fn table_job(
    text: String,
    font: &TableFont,
    color: Color32,
    wrap: Option<(f32, u8)>,
) -> egui::text::LayoutJob {
    let font_id = if font.is_default() {
        FontId::proportional(FONT_SIZE)
    } else {
        font.face.font(font.points, font.bold)
    };
    let mut job = egui::text::LayoutJob::single_section(
        text,
        egui::TextFormat {
            font_id,
            color,
            italics: font.italic,
            ..Default::default()
        },
    );
    match wrap {
        Some((width, lines)) => {
            job.wrap.max_width = width.max(1.);
            job.wrap.max_rows = if lines == 0 {
                usize::MAX
            } else {
                usize::from(lines)
            };
        }
        None => job.wrap.max_width = f32::INFINITY,
    }
    job
}

impl App {
    // ------------------------------------------------------------ toggles

    pub(crate) fn toggle_line_numbers(&mut self) {
        let t = &mut self.grid.layout.table;
        t.line_numbers = !t.line_numbers;
    }

    pub(crate) fn toggle_hint_help(&mut self) {
        let t = &mut self.grid.layout.table;
        t.hint_help = !t.hint_help;
        self.layout_opts.hint = None;
    }

    pub(crate) fn toggle_reorganize(&mut self) {
        let off = !self.layout_opts.reorganize_off;
        self.layout_opts.reorganize_off = off;
        if !off {
            self.layout_opts.frozen.borrow_mut().take();
        }
        self.status = if off {
            "Auto-reorganization disabled".into()
        } else {
            "Auto-reorganization enabled".into()
        };
    }

    pub(crate) fn toggle_spotlight(&mut self) {
        let on = !self.gantt.options.spotlight;
        self.gantt.options.spotlight = on;
        self.layout_opts.spotlight_end = None;
        if on && self.shown().model.project.data_date.is_none() {
            self.status = "Progress Spotlight needs a data date.".into();
        }
    }

    /// F5 and turning auto-reorganization back on regroup and resort.
    pub(crate) fn reorganize_now(&mut self) {
        self.layout_opts.frozen.borrow_mut().take();
    }

    // ---------------------------------------------------------- spotlight

    /// One interval of the lower timescale band after `dd`.
    pub(crate) fn spotlight_default_end(&self, dd: i64) -> i64 {
        let origin = self.shown().model.origin_serial;
        match self.view.lower {
            timescale::Unit::Hour => dd + 60,
            timescale::Unit::Day => dd + DAY,
            timescale::Unit::Week => dd + 7 * DAY,
            timescale::Unit::Month => add_months(origin, dd, 1),
            timescale::Unit::Quarter => add_months(origin, dd, 3),
            timescale::Unit::Year => add_months(origin, dd, 12),
        }
    }

    /// The spotlight period, data date to spotlight end, when it is on.
    pub(crate) fn spotlight_window(&self) -> Option<(i64, i64)> {
        if !self.gantt.options.spotlight {
            return None;
        }
        let dd = self.shown().model.project.data_date?;
        let end = self
            .layout_opts
            .spotlight_end
            .unwrap_or_else(|| self.spotlight_default_end(dd));
        Some((dd, end.max(dd)))
    }

    /// Activities that should progress in the spotlight period: open work
    /// scheduled to start before its end (work under way at the data date
    /// included). Indices into `shown()`.
    pub(crate) fn spotlight_activities(&self) -> BTreeSet<usize> {
        let Some((_, end)) = self.spotlight_window() else {
            return BTreeSet::new();
        };
        let m = &self.shown().model;
        let leveled = m.options.do_level;
        m.activities
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                !a.is_completed() && displayed_start(a, leveled).is_some_and(|s| s < end)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Spotlighted activities of the active project (Update Progress).
    pub(crate) fn spotlight_active(&self) -> BTreeSet<usize> {
        let n = self
            .active_rows()
            .unwrap_or(self.project.model.activities.len());
        self.spotlight_activities()
            .into_iter()
            .filter(|&i| i < n)
            .collect()
    }

    /// Dragging the data date line moves the spotlight's end (never before
    /// the data date), snapped to whole days.
    pub(crate) fn drag_spotlight_to(&mut self, minute: i64) {
        let Some(dd) = self.shown().model.project.data_date else {
            return;
        };
        let day = (minute as f64 / DAY as f64).round() as i64 * DAY;
        self.layout_opts.spotlight_end = Some(day.max(dd));
    }

    // -------------------------------------------------------- check filters

    pub(crate) fn check_filter_on(&self, filter: checks::CheckFilter) -> bool {
        let f = &self.grid.layout.filter;
        f.active() && f.selected == [filter.key()]
    }

    /// Show only the activities `filter` finds; again shows all activities.
    pub(crate) fn apply_check_filter(&mut self, filter: checks::CheckFilter) {
        let on = self.check_filter_on(filter);
        let f = &mut self.grid.layout.filter;
        if on {
            f.all_activities = true;
        } else {
            f.all_activities = false;
            f.selected = vec![filter.key()];
            f.match_all = true;
            f.highlight = false;
        }
        self.drop_frame_cache();
    }

    // ---------------------------------------------------------- Collapse To

    /// Outline levels of the grouped table, deepest last: the caption of
    /// each band level when every band is expanded.
    pub(crate) fn outline_levels(&self) -> Vec<String> {
        let ix = self.grid_index();
        let catalog = grid::columns::catalog(self.shown());
        let none = BTreeSet::new();
        let built = grid::group::build(
            self.shown(),
            &ix,
            &catalog,
            &self.grid.layout,
            &grid::group::Scope {
                query: &self.query,
                wbs_filter: self.wbs_filter,
                collapsed: &none,
                users: &self.grid.user_filters,
            },
        );
        let mut levels: Vec<String> = vec![];
        let mut wbs_level = 0;
        let depth = built.bands.iter().map(|b| b.level + 1).max().unwrap_or(0);
        for level in 0..depth {
            let Some(band) = built.bands.iter().find(|b| b.level == level) else {
                levels.push(format!("Level {}", level + 1));
                continue;
            };
            // The key's last part names the grouping: `w<id>|` for WBS,
            // `<column>=<value>|` otherwise.
            let last = band
                .key
                .trim_end_matches('|')
                .rsplit('|')
                .next()
                .unwrap_or("");
            let caption = match last.split_once('=') {
                Some((column, _)) => grid::columns::find(&catalog, column)
                    .map_or_else(|| column.to_string(), |c| c.title.clone()),
                None => {
                    wbs_level += 1;
                    format!("WBS Level {wbs_level}")
                }
            };
            levels.push(caption);
        }
        levels
    }

    /// Collapse To for the WBS or Projects window: the same dialog, over
    /// that window's tree levels.
    pub(crate) fn open_collapse_to_for(&mut self, target: CollapseTarget) {
        let levels = match target {
            CollapseTarget::Activities => return self.open_collapse_to(),
            CollapseTarget::Wbs => self.wbs_outline_levels(),
            CollapseTarget::Projects => self.projects_outline_levels(),
        };
        self.layout_opts.collapse_for = target;
        self.layout_opts.drafts.collapse = Some(dialogs::CollapseDraft {
            levels,
            selected: None,
        });
    }

    /// Show band levels 1 to `level` with the bands of `level` collapsed.
    pub(crate) fn collapse_to(&mut self, level: usize) {
        match self.layout_opts.collapse_for {
            CollapseTarget::Wbs => return self.wbs_collapse_to(level),
            CollapseTarget::Projects => return self.projects_collapse_to(level),
            CollapseTarget::Activities => {}
        }
        self.collapsed.clear();
        let _ = self.rows();
        self.collapsed = self
            .grid
            .bands
            .borrow()
            .iter()
            .filter(|b| b.level + 1 >= level.max(1))
            .map(|b| b.key.clone())
            .collect();
    }

    // ----------------------------------------------------------- New Layout

    /// Create layout `name` from the default layout or the current one and
    /// make it the working layout.
    pub(crate) fn new_layout(&mut self, name: &str, from_current: bool) -> Result<(), String> {
        let name = layout::check_name(name)?;
        if name == layout::STANDARD_NAME
            || self
                .grid
                .library
                .iter()
                .any(|l| l.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("A layout named {name} already exists."));
        }
        if from_current {
            return self.save_layout_as(&name);
        }
        let mut fresh = layout::Layout {
            name: name.clone(),
            ..Default::default()
        };
        fresh.gantt = Some(serde_json::json!({
            "settings": serde_json::from_str::<serde_json::Value>(
                &gantt::settings::GanttSettings::default().to_json()
            ).ok(),
        }));
        let checked = layout::Layout::from_json(&fresh.to_json())?;
        self.grid.library.push(checked);
        self.save_library();
        self.open_layout(&name, true)?;
        self.status = format!("Layout {name} created");
        Ok(())
    }

    // ------------------------------------------------------------ table rows

    /// Activity row pitch: the Gantt's row height, never less than the
    /// table font needs.
    pub(crate) fn activity_row_height(&self) -> f32 {
        let font = &self.grid.layout.table.font;
        let base = self.gantt_row_height();
        if font.is_default() {
            base
        } else {
            base.max(layout::font_row_height(font.points))
        }
    }

    /// The height of activity row `i`: taller when its text wraps.
    pub(crate) fn activity_height(&self, i: usize) -> f32 {
        let base = self.activity_row_height();
        match self.layout_opts.heights.borrow().as_ref() {
            Some((_, heights)) if self.grid.layout.table.optimize_rows.is_some() => {
                heights.get(&i).map_or(base, |h| h.max(base))
            }
            _ => base,
        }
    }

    /// Optimize height by row content: measure each activity's wrapped text
    /// columns once per change of the project, columns or font.
    pub(crate) fn measure_row_heights(&self, ctx: &egui::Context) {
        let table = &self.grid.layout.table;
        let Some(lines) = table.optimize_rows else {
            return;
        };
        let serial = self.shown_serial();
        let key = serial.map(|serial| HeightKey {
            serial,
            columns: self.grid.layout.columns.clone(),
            font: table.font.clone(),
            lines,
            formats: self.formats.clone(),
        });
        if let (Some(key), Some((stored, _))) = (&key, self.layout_opts.heights.borrow().as_ref()) {
            if key == stored {
                return;
            }
        }
        let shown = self.shown();
        let catalog = grid::columns::catalog(shown);
        let cols = self.visible_columns(&catalog);
        let ix = self.grid_index();
        let font = table.font.clone();
        let mut heights = HashMap::new();
        let base = self.activity_row_height();
        ctx.fonts_mut(|f| {
            for i in 0..shown.model.activities.len() {
                let mut tallest: f32 = 0.;
                for c in cols.iter().filter(|c| c.kind == grid::columns::Kind::Text) {
                    let text = grid::columns::display(
                        shown,
                        Some(i),
                        &grid::columns::shown_value(shown, &ix, c, i),
                    );
                    if text.is_empty() {
                        continue;
                    }
                    let job = table_job(text, &font, INK, Some((c.width - 7., lines)));
                    tallest = tallest.max(f.layout_job(job).size().y);
                }
                // Text from 3 px below the row top, 3 px under it and the line.
                let needed = (tallest + 7.).ceil();
                if needed > base {
                    heights.insert(i, needed);
                }
            }
        });
        *self.layout_opts.heights.borrow_mut() = Some((
            key.unwrap_or(HeightKey {
                serial: u64::MAX,
                columns: vec![],
                font,
                lines,
                formats: self.formats.clone(),
            }),
            Rc::new(heights),
        ));
    }

    /// Width of the line number gutter for `rows` rows.
    pub(crate) fn line_number_width(&self, rows: usize) -> f32 {
        if !self.grid.layout.table.line_numbers {
            return 0.;
        }
        let digits = rows.max(1).to_string().len() as f32;
        (digits * 6. + 12.).max(28.).round()
    }

    /// One row's line number cell: face fill, the number right aligned.
    pub(crate) fn paint_line_number(&self, p: &egui::Painter, rect: Rect, number: usize) {
        let rect = Rect::from_min_max(rect.min.round(), rect.max.round());
        chrome::fill(p, rect, chrome::pal::FACE);
        chrome::hline(
            p,
            rect.left(),
            rect.right(),
            rect.bottom() - 1.,
            Color32::from_rgb(0xC0, 0xC0, 0xC0),
        );
        chrome::vline(p, rect.right() - 1., rect.top(), rect.bottom(), INK);
        let galley = p.layout_no_wrap(number.to_string(), FontId::proportional(FONT_SIZE), INK);
        let x = rect.right() - 5. - galley.size().x;
        p.with_clip_rect(rect)
            .galley(Pos2::new(x.round(), rect.top() + 3.), galley, INK);
    }

    /// The gutter's header cell: the raised header face.
    pub(crate) fn paint_line_number_header(&self, p: &egui::Painter, rect: Rect) {
        let rect = Rect::from_min_max(rect.min.round(), rect.max.round());
        chrome::fill(p, rect, chrome::pal::FACE);
        chrome::hline(p, rect.left(), rect.right(), rect.top(), Color32::WHITE);
        chrome::vline(p, rect.left(), rect.top(), rect.bottom(), Color32::WHITE);
        chrome::hline(p, rect.left(), rect.right(), rect.bottom() - 1., INK);
        chrome::vline(p, rect.right() - 1., rect.top(), rect.bottom(), INK);
    }

    // ------------------------------------------------------ reorganization

    fn freeze_key(&self) -> FreezeKey {
        FreezeKey {
            project: self.shown().model.project.proj_id,
            multi: self.active_rows().is_some(),
            filter: self.grid.layout.filter.clone(),
            group_sort: self.grid.layout.group_sort.clone(),
            users: self.grid.user_filters.clone(),
            query: self.query.clone(),
            wbs_filter: self.wbs_filter,
            collapsed: self.collapsed.clone(),
        }
    }

    /// While auto-reorganization is off, edits do not move rows: the rows
    /// keep the order they had when it was turned off (or when the layout,
    /// filter, grouping or collapsed bands last changed). Deleted
    /// activities drop out; new ones appear after the row they follow in
    /// the regular order.
    /// `reorganized` that hands `fresh` back untouched while rows keep the
    /// regular order, so the cached row build is not copied.
    pub(crate) fn reorganized_shared(&self, fresh: std::rc::Rc<Vec<Row>>) -> std::rc::Rc<Vec<Row>> {
        if !self.layout_opts.reorganize_off && !self.layout_opts.bands_frozen.get() {
            self.layout_opts.frozen.borrow_mut().take();
            return fresh;
        }
        std::rc::Rc::new(self.reorganized(std::rc::Rc::unwrap_or_clone(fresh)))
    }

    pub(crate) fn reorganized(&self, fresh: Vec<Row>) -> Vec<Row> {
        if !self.layout_opts.reorganize_off {
            self.layout_opts.frozen.borrow_mut().take();
            return self.unfreeze_bands(fresh);
        }
        let key = self.freeze_key();
        let acts = &self.shown().model.activities;
        let mut frozen = self.layout_opts.frozen.borrow_mut();
        let Some(f) = frozen.as_mut().filter(|f| f.key == key) else {
            // Start a new frozen order from a regular build.
            drop(frozen);
            let fresh = self.unfreeze_bands(fresh);
            let bands = self.grid.bands.borrow().clone();
            *self.layout_opts.frozen.borrow_mut() = Some(Frozen {
                key,
                rows: fresh
                    .iter()
                    .map(|r| match *r {
                        Row::Group(b) => FrozenRow::Group(b),
                        Row::Activity(i) => FrozenRow::Activity(acts[i].task_id),
                    })
                    .collect(),
                members: bands
                    .iter()
                    .map(|b| b.members.iter().map(|&i| acts[i].task_id).collect())
                    .collect(),
                bands,
            });
            return fresh;
        };
        let index: HashMap<i64, usize> = acts
            .iter()
            .enumerate()
            .map(|(i, a)| (a.task_id, i))
            .collect();
        let mut out: Vec<Row> = vec![];
        let mut placed: BTreeSet<usize> = BTreeSet::new();
        for row in &f.rows {
            match *row {
                FrozenRow::Group(b) => out.push(Row::Group(b)),
                FrozenRow::Activity(t) => {
                    if let Some(&i) = index.get(&t) {
                        if placed.insert(i) {
                            out.push(Row::Activity(i));
                        }
                    }
                }
            }
        }
        // New activities follow the row before them in the regular order.
        let mut after: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
        let mut anchor = None;
        for row in &fresh {
            if let Row::Activity(i) = *row {
                if placed.contains(&i) {
                    anchor = Some(i);
                } else {
                    after.entry(anchor).or_default().push(i);
                    placed.insert(i);
                    anchor = Some(i);
                }
            }
        }
        if !after.is_empty() {
            let mut merged = Vec::with_capacity(out.len() + after.len());
            for row in out {
                merged.push(row);
                if let Row::Activity(i) = row {
                    if let Some(new) = after.remove(&Some(i)) {
                        merged.extend(new.into_iter().map(Row::Activity));
                    }
                }
            }
            for new in after.into_values() {
                merged.extend(new.into_iter().map(Row::Activity));
            }
            out = merged;
            f.rows = out
                .iter()
                .map(|r| match *r {
                    Row::Group(b) => FrozenRow::Group(b),
                    Row::Activity(i) => FrozenRow::Activity(acts[i].task_id),
                })
                .collect();
        }
        let bands: Vec<grid::group::Band> = f
            .bands
            .iter()
            .zip(&f.members)
            .map(|(b, members)| grid::group::Band {
                members: members
                    .iter()
                    .filter_map(|t| index.get(t).copied())
                    .collect::<Vec<_>>()
                    .into(),
                ..b.clone()
            })
            .collect();
        *self.grid.bands.borrow_mut() = bands;
        self.layout_opts.bands_frozen.set(true);
        out
    }

    /// Rows as built, after the frozen bands are replaced by a fresh build.
    fn unfreeze_bands(&self, fresh: Vec<Row>) -> Vec<Row> {
        if !self.layout_opts.bands_frozen.replace(false) {
            return fresh;
        }
        self.grid.drop_rows_cache();
        self.built_rows()
    }
}

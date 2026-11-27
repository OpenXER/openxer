//! Turning the activity list into table rows: filter, multi-column sort and
//! nested grouping bands (WBS hierarchy, codes or any column).
use super::columns::{self, Column, Field, Index, Kind, Source, Value};
use super::filters::{self, UserFilter};
use super::layout::{GroupLevel, GroupSort, Layout};
use super::*;
use crate::format::DateMark;
use std::cmp::Ordering;
use std::rc::Rc;

/// The activities under a band: a range of one shared list, so nested WBS
/// bands share storage instead of each copying its whole subtree.
#[derive(Clone, Debug, PartialEq)]
pub struct Members {
    list: Rc<[usize]>,
    range: std::ops::Range<usize>,
}

impl Members {
    fn shared(list: Rc<[usize]>, range: std::ops::Range<usize>) -> Self {
        Self { list, range }
    }

    /// Identity of this member list: the shared list's address and the
    /// range. Equal identities hold the same activities while either list
    /// is alive.
    fn identity(&self) -> (usize, usize, usize) {
        (
            Rc::as_ptr(&self.list) as *const usize as usize,
            self.range.start,
            self.range.end,
        )
    }
}

impl From<Vec<usize>> for Members {
    fn from(v: Vec<usize>) -> Self {
        let range = 0..v.len();
        Self {
            list: v.into(),
            range,
        }
    }
}

impl std::ops::Deref for Members {
    type Target = [usize];
    fn deref(&self) -> &[usize] {
        &self.list[self.range.clone()]
    }
}

/// What activities dropped on (or added under) a band take from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandTarget {
    /// A WBS band: the element's `wbs_id`.
    Wbs(i64),
    /// An activity code band; `None` for the band of activities without a value.
    Code { type_id: i64, value: Option<i64> },
    /// Any other band (status, dates, totals, unassigned WBS).
    Other,
}

/// A grouping band row.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// Collapse key, unique within one build.
    pub key: String,
    /// Nesting depth from 0; selects the band style.
    pub level: usize,
    pub label: String,
    /// Every activity shown under the band (before collapsing).
    pub members: Members,
    pub target: BandTarget,
}

/// Rows of the table in display order plus the bands they reference.
#[derive(Clone, Debug, Default)]
pub struct Built {
    pub rows: Vec<Row>,
    pub bands: Vec<Band>,
    /// Activities that match the filter when the filter highlights instead of hiding.
    pub highlighted: BTreeSet<usize>,
}

/// Inputs to a build that are not part of the layout.
pub struct Scope<'a> {
    pub query: &'a str,
    pub wbs_filter: Option<i64>,
    pub collapsed: &'a BTreeSet<String>,
    pub users: &'a [UserFilter],
}

/// Sort activities by the layout's sort keys; ties keep list order.
pub fn sort(
    p: &ProjectData,
    ix: &Index,
    catalog: &[Column],
    keys: &[layout::SortKey],
    list: &mut [usize],
) {
    let resolved: Vec<(&Column, bool)> = keys
        .iter()
        .filter_map(|k| columns::find(catalog, &k.column).map(|c| (c, k.descending)))
        .collect();
    if resolved.is_empty() {
        return;
    }
    let mut keyed: Vec<(Vec<Value>, usize)> = list
        .iter()
        .map(|&i| {
            (
                resolved
                    .iter()
                    .map(|(c, _)| columns::value(p, ix, c, i))
                    .collect(),
                i,
            )
        })
        .collect();
    keyed.sort_by(|a, b| {
        for (n, (_, descending)) in resolved.iter().enumerate() {
            let o = columns::compare(&a.0[n], &b.0[n]);
            let o = if *descending { o.reverse() } else { o };
            if o != Ordering::Equal {
                return o;
            }
        }
        a.1.cmp(&b.1)
    });
    for (slot, (_, i)) in list.iter_mut().zip(keyed) {
        *slot = i;
    }
}

struct Builder<'a> {
    p: &'a ProjectData,
    ix: &'a Index,
    catalog: &'a [Column],
    gs: &'a GroupSort,
    collapsed: &'a BTreeSet<String>,
    out: Built,
}

impl Builder<'_> {
    fn label(&self, title: &str, code: &str, name: &str) -> String {
        // The layout's Group and Sort choices, within User Preferences.
        let (code_on, name_on) = crate::format::band_parts();
        let show_code = self.gs.show_code && code_on;
        let show_name = self.gs.show_name && name_on;
        let mut parts = vec![];
        if self.gs.show_title {
            parts.push(format!("{title}:"));
        }
        if show_code && !code.is_empty() {
            parts.push(code.to_string());
        }
        if show_name && !name.is_empty() && (name != code || !show_code) {
            parts.push(name.to_string());
        }
        if parts.is_empty() || parts.len() == 1 && self.gs.show_title {
            parts.push(if name.is_empty() { code } else { name }.to_string());
        }
        parts.join("  ")
    }

    /// Push a band; returns whether its contents are shown.
    fn band(
        &mut self,
        key: String,
        level: usize,
        label: String,
        members: impl Into<Members>,
        target: BandTarget,
    ) -> bool {
        let open = !self.collapsed.contains(&key);
        self.out.bands.push(Band {
            key,
            level,
            label,
            members: members.into(),
            target,
        });
        self.out.rows.push(Row::Group(self.out.bands.len() - 1));
        open
    }

    fn leaf(&mut self, members: &[usize]) {
        if !self.gs.show_summaries_only {
            self.out
                .rows
                .extend(members.iter().copied().map(Row::Activity));
        }
    }

    fn group(&mut self, level_index: usize, members: Vec<usize>, prefix: &str, depth: usize) {
        let Some(level) = self.gs.levels.get(level_index).cloned() else {
            self.leaf(&members);
            return;
        };
        if level.column == "wbs" {
            self.wbs(level_index, &level, members, prefix, depth);
            return;
        }
        let Some(column) = columns::find(self.catalog, &level.column).cloned() else {
            self.group(level_index + 1, members, prefix, depth);
            return;
        };
        let interval = super::interval::parse(self.p, column.kind, &level.interval);
        // (order key, code, name) per band, members in sorted order.
        let mut bands: Vec<(Value, i64, String, String, Vec<usize>, BandTarget)> = vec![];
        let mut by_code: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for i in members {
            let task_id = self.p.model.activities[i].task_id;
            let (value, seq, code, name, target) = match column.source {
                Source::Code(type_id) => {
                    let value = self.ix.code_id(self.p, task_id, type_id);
                    let target = BandTarget::Code { type_id, value };
                    match self.ix.code_of(self.p, task_id, type_id) {
                        Some(c) => (
                            Value::Text(c.short_name.clone()),
                            c.sequence,
                            c.short_name,
                            c.name,
                            target,
                        ),
                        None => (Value::Empty, i64::MAX, String::new(), String::new(), target),
                    }
                }
                _ => {
                    // Dates group by day unless an interval widens the bands.
                    let v = columns::value(self.p, self.ix, &column, i);
                    let (grouped, text) = super::interval::bucket(self.p, &v, interval, Some(i));
                    (grouped, 0, text.clone(), text, BandTarget::Other)
                }
            };
            let existing = by_code
                .get(&code)
                .and_then(|indices| indices.iter().copied().find(|&idx| bands[idx].0 == value));
            match existing {
                Some(idx) => bands[idx].4.push(i),
                None => {
                    let idx = bands.len();
                    by_code.entry(code.clone()).or_default().push(idx);
                    bands.push((value, seq, code, name, vec![i], target));
                }
            }
        }
        let alpha = self.gs.sort_bands_alphabetically;
        if alpha {
            let mut order: Vec<(usize, String)> = bands
                .iter()
                .enumerate()
                .map(|(idx, b)| (idx, b.3.to_lowercase()))
                .collect();
            order.sort_by(|&(ia, ref ka), &(ib, ref kb)| {
                let blank = |v: &Value| matches!(v, Value::Empty);
                blank(&bands[ia].0)
                    .cmp(&blank(&bands[ib].0))
                    .then_with(|| ka.cmp(kb))
            });
            let mut slots: Vec<_> = bands.into_iter().map(Some).collect();
            bands = order
                .into_iter()
                .map(|(idx, _)| slots[idx].take().unwrap())
                .collect();
        } else {
            bands.sort_by(|a, b| {
                let blank = |v: &Value| matches!(v, Value::Empty);
                blank(&a.0)
                    .cmp(&blank(&b.0))
                    .then_with(|| a.1.cmp(&b.1).then_with(|| columns::compare(&a.0, &b.0)))
            });
        }
        for (value, _, code, name, list, target) in bands {
            let key = format!("{prefix}{}={}|", column.id, code);
            let (code, name) = if matches!(value, Value::Empty) {
                (String::new(), format!("No {}", column.title))
            } else {
                (code, name)
            };
            let label = self.label(&column.title, &code, &name);
            if self.band(key.clone(), depth, label, list.clone(), target) {
                self.group(level_index + 1, list, &key, depth + 1);
            }
        }
    }

    fn wbs(
        &mut self,
        level_index: usize,
        level: &GroupLevel,
        members: Vec<usize>,
        prefix: &str,
        depth: usize,
    ) {
        let (ix, p) = (self.ix, self.p);
        let w = ix.wbs(p);
        let tree = &w.tree;
        let mut direct: BTreeMap<Option<i64>, Vec<usize>> = BTreeMap::new();
        for i in members {
            let id = self
                .p
                .details
                .get(&self.p.model.activities[i].task_id)
                .map(|d| d.wbs)
                .filter(|id| tree.parent.contains_key(id));
            direct.entry(id).or_default().push(i);
        }
        // One preorder walk, no recursion: WBS depth comes from the file.
        // Laying each node's own activities out in preorder makes every
        // subtree's activities one contiguous range of `order`.
        let preorder = tree.preorder();
        let mut order: Vec<usize> = vec![];
        let mut starts = Vec::with_capacity(preorder.len());
        for &(id, _) in &preorder {
            starts.push(order.len());
            if let Some(own) = direct.get(&Some(id)) {
                order.extend_from_slice(own);
            }
        }
        // A subtree ends where the next node at the same or a shallower depth starts.
        let mut ends = vec![order.len(); preorder.len()];
        let mut open: Vec<usize> = vec![];
        for (k, &(_, d)) in preorder.iter().enumerate() {
            while open.last().is_some_and(|&j| preorder[j].1 >= d) {
                ends[open.pop().unwrap()] = starts[k];
            }
            open.push(k);
        }
        let order: Rc<[usize]> = order.into();
        let limit = level.to_level.unwrap_or(usize::MAX);
        // Descendants of a node deeper than this are not shown.
        let mut skip_below: Option<usize> = None;
        for (k, &(id, d)) in preorder.iter().enumerate() {
            if skip_below.is_some_and(|s| d > s) {
                continue;
            }
            skip_below = None;
            if d >= limit {
                continue;
            }
            let members = Members::shared(order.clone(), starts[k]..ends[k]);
            if members.is_empty() && self.gs.hide_if_empty {
                skip_below = Some(d);
                continue;
            }
            // WBS ids are unique, so the key needs only the outer prefix.
            let key = format!("{prefix}w{id}|");
            let code = if self.gs.show_code {
                w.codes.get(&id)
            } else {
                w.short_codes.get(&id)
            };
            let code = code.map_or("", String::as_str);
            let label = self.label("WBS", code, w.names.get(&id).map_or("", String::as_str));
            let band_depth = depth + d;
            if !self.band(
                key.clone(),
                band_depth,
                label,
                members.clone(),
                BandTarget::Wbs(id),
            ) {
                skip_below = Some(d);
                continue;
            }
            if d + 1 >= limit {
                // The deepest shown level holds its whole subtree.
                self.group(level_index + 1, members.to_vec(), &key, band_depth + 1);
                skip_below = Some(d);
                continue;
            }
            if let Some(own) = direct.get(&Some(id)) {
                if !own.is_empty() {
                    self.group(level_index + 1, own.clone(), &key, band_depth + 1);
                }
            }
        }
        if let Some(loose) = direct.get(&None).cloned() {
            let key = format!("{prefix}w0|");
            let label = self.label("WBS", "", "Unassigned");
            if self.band(key.clone(), depth, label, loose.clone(), BandTarget::Other) {
                self.group(level_index + 1, loose, &key, depth + 1);
            }
        }
    }
}

/// Build the table rows for a layout.
pub fn build(
    p: &ProjectData,
    ix: &Index,
    catalog: &[Column],
    layout: &Layout,
    scope: &Scope,
) -> Built {
    let q = scope.query.to_lowercase();
    if let Some(paths) = &ix.critical_paths {
        return critical_paths(p, ix, catalog, layout, scope, paths);
    }
    let filter = &layout.filter;
    let mut highlighted = BTreeSet::new();
    let selection = filters::PreparedSelection::new(p, catalog, filter, scope.users);
    let mut members: Vec<usize> = (0..p.model.activities.len())
        .filter(|&i| {
            let a = &p.model.activities[i];
            if let Some(id) = scope.wbs_filter {
                if p.details.get(&a.task_id).map_or(0, |d| d.wbs) != id {
                    return false;
                }
            }
            if !q.is_empty()
                && !a.task_code.to_lowercase().contains(&q)
                && !p.name(i).to_lowercase().contains(&q)
            {
                return false;
            }
            let pass = selection.matches(p, ix, i);
            if filter.highlight {
                if pass && filter.active() {
                    highlighted.insert(i);
                }
                true
            } else {
                pass
            }
        })
        .collect();
    sort(p, ix, catalog, &layout.group_sort.sort, &mut members);
    let mut b = Builder {
        p,
        ix,
        catalog,
        gs: &layout.group_sort,
        collapsed: scope.collapsed,
        out: Built::default(),
    };
    if layout.group_sort.show_grand_totals {
        if b.band(
            "grand|".into(),
            0,
            "Total".into(),
            members.clone(),
            BandTarget::Other,
        ) {
            b.group(0, members, "grand|", 1);
        }
    } else {
        b.group(0, members, "", 0);
    }
    b.out.highlighted = highlighted;
    b.out
}

/// `band_total`, computed once per column and member list for the lookups
/// in `ix`: the table draws band totals every frame, and with earned value
/// columns a total over a whole project's activities costs milliseconds.
/// `ix` must have been built for `p` as it is now (as the table's are).
pub fn band_total_cached(
    p: &ProjectData,
    ix: &Index,
    column: &Column,
    members: &Members,
) -> Option<Value> {
    let (ptr, start, end) = members.identity();
    let key = (column.id.clone(), ptr, start, end);
    if let Some((_, v)) = ix.band_totals.borrow().get(&key) {
        return v.clone();
    }
    let v = band_total(p, ix, column, members);
    let mut totals = ix.band_totals.borrow_mut();
    // Frozen row orders rebuild their member lists; keep the memo bounded.
    if totals.len() > 50_000 {
        totals.clear();
    }
    totals.insert(key, (members.clone(), v.clone()));
    v
}

/// View > Activity Critical Path: only the activities on the shown paths
/// (the text search and WBS filter still apply). With Enable Group and Sort
/// each path is a band, most critical first, its activities in path order;
/// otherwise the layout's own grouping and sorting arrange them.
fn critical_paths(
    p: &ProjectData,
    ix: &Index,
    catalog: &[Column],
    layout: &Layout,
    scope: &Scope,
    paths: &crate::critical_path::PathView,
) -> Built {
    let q = scope.query.to_lowercase();
    let mut members: Vec<usize> = (0..p.model.activities.len())
        .filter(|i| paths.rows.contains_key(i))
        .filter(|&i| {
            let a = &p.model.activities[i];
            scope
                .wbs_filter
                .is_none_or(|id| p.details.get(&a.task_id).map_or(0, |d| d.wbs) == id)
                && (q.is_empty()
                    || a.task_code.to_lowercase().contains(&q)
                    || p.name(i).to_lowercase().contains(&q))
        })
        .collect();
    let mut b = Builder {
        p,
        ix,
        catalog,
        gs: &layout.group_sort,
        collapsed: scope.collapsed,
        out: Built::default(),
    };
    if !paths.group {
        sort(p, ix, catalog, &layout.group_sort.sort, &mut members);
        b.group(0, members, "", 0);
        return b.out;
    }
    members.sort_by_key(|i| paths.rows[i]);
    for (n, label) in paths.labels.iter().enumerate() {
        let list: Vec<usize> = members
            .iter()
            .copied()
            .filter(|i| paths.rows[i].0 == n)
            .collect();
        if list.is_empty() {
            continue;
        }
        let key = format!("acp={n}|");
        if b.band(key, 0, label.clone(), list.clone(), BandTarget::Other) {
            b.leaf(&list);
        }
    }
    b.out
}

/// The band row's total for a column, when the column has one: earliest
/// start-type date, latest finish-type date, the span's Original Duration,
/// smallest float, summed units and costs.
pub fn band_total(
    p: &ProjectData,
    ix: &Index,
    column: &Column,
    members: &[usize],
) -> Option<Value> {
    let Source::Field(field) = column.source else {
        return None;
    };
    if let Field::Ext(e) = field {
        return super::columns_ext::band_total(p, ix, e, members);
    }
    let values = || members.iter().map(|&i| columns::value(p, ix, column, i));
    // A band keeps the actual marker of its earliest start (or of a finish
    // when every member finished); the constraint marker belongs to one
    // activity and never shows on a band.
    let actual_mark = |actual: bool| {
        if actual {
            DateMark::Actual
        } else {
            DateMark::Plain
        }
    };
    let earliest = |actual_ok: bool| {
        values()
            .filter_map(|v| match v {
                Value::Date { minute, mark } => {
                    Some((minute, mark == DateMark::Actual && actual_ok))
                }
                _ => None,
            })
            .min_by_key(|v| v.0)
            .map(|(minute, actual)| Value::Date {
                minute,
                mark: actual_mark(actual),
            })
    };
    let latest = || {
        let all: Vec<Value> = values().collect();
        let complete = all.iter().all(|v| {
            matches!(
                v,
                Value::Date {
                    mark: DateMark::Actual,
                    ..
                }
            )
        });
        all.into_iter()
            .filter_map(|v| match v {
                Value::Date { minute, .. } => Some(minute),
                _ => None,
            })
            .max()
            .map(|minute| Value::Date {
                minute,
                mark: actual_mark(complete),
            })
    };
    match field {
        Field::Start => earliest(true),
        Field::EarlyStart
        | Field::LateStart
        | Field::ActualStart
        | Field::PlannedStart
        | Field::RemainingEarlyStart
        | Field::RemainingLateStart => earliest(false),
        Field::Finish => latest(),
        Field::EarlyFinish
        | Field::LateFinish
        | Field::ActualFinish
        | Field::PlannedFinish
        | Field::RemainingEarlyFinish
        | Field::RemainingLateFinish => latest().map(|v| match v {
            Value::Date { minute, .. } => Value::Date {
                minute,
                mark: DateMark::Plain,
            },
            other => other,
        }),
        // The band's span: working time on the project calendar from its
        // earliest planned start to its latest planned finish.
        Field::OriginalDuration => {
            let acts = &p.model.activities;
            let start = members
                .iter()
                .filter_map(|&i| acts.get(i).and_then(|a| a.target_start.or(a.early_start)))
                .min()?;
            let finish = members
                .iter()
                .filter_map(|&i| acts.get(i).and_then(|a| a.target_finish.or(a.early_finish)))
                .max()?;
            let span = p
                .model
                .calendars
                .get(p.model.project_calendar)
                .map_or(finish - start, |cal| cal.between(start, finish));
            Some(Value::Duration(span.max(0)))
        }
        Field::TotalFloat | Field::FreeFloat => values()
            .filter_map(|v| match v {
                Value::Duration(m) => Some(m),
                _ => None,
            })
            .min()
            .map(Value::Duration),
        _ if matches!(column.kind, Kind::Units | Kind::Cost) => {
            let sum: f64 = values().filter_map(|v| v.number()).sum();
            Some(if column.kind == Kind::Cost {
                Value::Cost(sum)
            } else {
                Value::Units(sum)
            })
        }
        _ => None,
    }
}

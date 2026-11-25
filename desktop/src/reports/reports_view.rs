//! The Reports view (Enterprise bar, Tools > Reports > Reports): a table of
//! the saved reports under their report groups, as a main view with its own
//! tab like Projects and Resources. Report groups are blue band rows to any
//! depth (reports with no group sit under a "No Report Group" band); the
//! columns are Report Name (with an icon for how the report was made),
//! Report Scope and Last Run Date, and Report Group on request. There is no
//! details pane. The report commands (run, modify, import, export, cut, copy,
//! paste, scope, group lookup) come from the Reports toolbar, the Edit
//! commands and the right-click menu, and all go through
//! [`App::reports_command`].
use super::groups::{self, Group};
use super::rpt::store::ReportScope;
use super::rpt_entries::{last_run_text, Entry, Which};
use crate::chrome::Icon;
use crate::context_menu::{Entry as MenuEntry, Menu, Spec};
use crate::dialog_kit as kit;
use crate::views::grid::{self, Columns, Row};
use crate::views::sheet;
use crate::App;
use eframe::egui::{self, Pos2, Rect, Vec2};
use std::collections::BTreeSet;

pub const TITLES: [&str; 4] = [
    "Report Name",
    "Report Scope",
    "Last Run Date",
    "Report Group",
];
const WIDTHS: [f32; 4] = [380., 110., 150., 240.];
/// Field of the Report Group column, which starts hidden.
pub const GROUP_FIELD: usize = 3;

/// How the rows are grouped into bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupBy {
    ReportGroup,
    Scope,
    Nothing,
}

impl GroupBy {
    fn key(self) -> &'static str {
        match self {
            GroupBy::ReportGroup => "report_group",
            GroupBy::Scope => "scope",
            GroupBy::Nothing => "none",
        }
    }
    fn from_key(k: &str) -> Option<Self> {
        [GroupBy::ReportGroup, GroupBy::Scope, GroupBy::Nothing]
            .into_iter()
            .find(|g| g.key() == k)
    }
}

/// The order of the reports inside a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Scope,
    LastRun,
}

impl SortBy {
    fn key(self) -> &'static str {
        match self {
            SortBy::Name => "name",
            SortBy::Scope => "scope",
            SortBy::LastRun => "last_run",
        }
    }
    fn from_key(k: &str) -> Option<Self> {
        [SortBy::Name, SortBy::Scope, SortBy::LastRun]
            .into_iter()
            .find(|g| g.key() == k)
    }
}

/// Which reports the view lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Global,
    Project,
    Wizard,
    Editor,
}

impl Filter {
    fn key(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Global => "global",
            Filter::Project => "project",
            Filter::Wizard => "wizard",
            Filter::Editor => "editor",
        }
    }
    fn from_key(k: &str) -> Option<Self> {
        [
            Filter::All,
            Filter::Global,
            Filter::Project,
            Filter::Wizard,
            Filter::Editor,
        ]
        .into_iter()
        .find(|g| g.key() == k)
    }
    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All Reports",
            Filter::Global => "Global Reports",
            Filter::Project => "Project Reports",
            Filter::Wizard => "Wizard Reports",
            Filter::Editor => "Editor Reports",
        }
    }
    fn keeps(self, e: &Entry) -> bool {
        match self {
            Filter::All => true,
            Filter::Global => e.scope.is_global(),
            Filter::Project => !e.scope.is_global(),
            Filter::Wizard => e.is_wizard(),
            Filter::Editor => matches!(e.which, Which::Stored(_)),
        }
    }
}

/// A row of the view: a band or a report.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowKey {
    Group(u32),
    /// The band of reports that have no report group.
    NoGroup,
    /// A band when grouped by scope (true: global).
    ScopeBand(bool),
    Report(Which),
}

impl RowKey {
    /// The text the collapsed set keeps for a band.
    fn band_key(&self) -> Option<String> {
        match self {
            RowKey::Group(id) => Some(format!("g{id}")),
            RowKey::NoGroup => Some("none".into()),
            RowKey::ScopeBand(global) => Some(format!(
                "scope-{}",
                if *global { "global" } else { "project" }
            )),
            RowKey::Report(_) => None,
        }
    }
}

pub struct ReportsViewState {
    pub columns: Columns,
    pub selected: Option<RowKey>,
    /// The row selection, one row at a time: the commands act on the
    /// current row only.
    pub rows: grid::RowSelection<RowKey>,
    /// Bands that are collapsed (see `RowKey::band_key`).
    pub collapsed: BTreeSet<String>,
    pub group_by: GroupBy,
    pub sort: SortBy,
    pub filter: Filter,
}

impl Default for ReportsViewState {
    fn default() -> Self {
        let mut columns = Columns::new(&WIDTHS);
        columns.order = vec![0, 1, 2];
        Self {
            columns,
            selected: None,
            rows: grid::RowSelection::single(),
            collapsed: BTreeSet::new(),
            group_by: GroupBy::ReportGroup,
            sort: SortBy::Name,
            filter: Filter::All,
        }
    }
}

impl ReportsViewState {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "columns": self.columns.json(),
            "collapsed": self.collapsed,
            "group_by": self.group_by.key(),
            "sort": self.sort.key(),
            "filter": self.filter.key(),
        })
    }

    pub fn restore(&mut self, value: &serde_json::Value) {
        self.columns.restore(&value["columns"]);
        if let Some(list) = value["collapsed"].as_array() {
            self.collapsed = list
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        if let Some(g) = value["group_by"].as_str().and_then(GroupBy::from_key) {
            self.group_by = g;
        }
        if let Some(s) = value["sort"].as_str().and_then(SortBy::from_key) {
            self.sort = s;
        }
        if let Some(f) = value["filter"].as_str().and_then(Filter::from_key) {
            self.filter = f;
        }
    }

    pub fn group_column_shown(&self) -> bool {
        self.columns.order.contains(&GROUP_FIELD)
    }

    fn toggle_band(&mut self, key: &RowKey) {
        if let Some(k) = key.band_key() {
            if !self.collapsed.remove(&k) {
                self.collapsed.insert(k);
            }
        }
    }
}

/// One row of the table with what it stands for.
pub struct Line {
    pub key: RowKey,
    pub row: Row,
}

/// The rows of the view for `entries` under `groups`: filtered, sorted and
/// banded as the state says, minus what a collapsed band hides.
pub fn build_lines(entries: &[Entry], groups: &[Group], st: &ReportsViewState) -> Vec<Line> {
    let shown: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| st.filter.keeps(e))
        .map(|(i, _)| i)
        .collect();
    let order = |mut list: Vec<usize>| {
        list.sort_by(|&a, &b| {
            let (x, y) = (&entries[a], &entries[b]);
            let by_name = x.name.to_lowercase().cmp(&y.name.to_lowercase());
            match st.sort {
                SortBy::Name => by_name,
                SortBy::Scope => y.scope.is_global().cmp(&x.scope.is_global()).then(by_name),
                // The newest run first; reports that never ran last.
                SortBy::LastRun => y.last_run.cmp(&x.last_run).then(by_name),
            }
        });
        list
    };
    let report = |i: usize, level: usize| {
        let e = &entries[i];
        Line {
            key: RowKey::Report(e.which.clone()),
            row: Row {
                depth: level,
                icon: Some(match e.which {
                    Which::Wizard(_) => Icon::WizardReport,
                    Which::Stored(_) => Icon::EditorReport,
                    Which::Builtin(_) => Icon::ActivityUsageProfile,
                }),
                cells: vec![
                    e.name.clone(),
                    e.scope.caption().into(),
                    last_run_text(e.last_run.as_deref()),
                    groups::group_path(groups, e.group),
                ],
                strips: level,
                ..Default::default()
            },
        }
    };
    let band = |key: RowKey, caption: String, level: usize, st: &ReportsViewState| {
        let open = key.band_key().is_none_or(|k| !st.collapsed.contains(&k));
        (
            open,
            Line {
                key,
                row: Row {
                    depth: level,
                    expander: Some(open),
                    cells: vec![caption, String::new(), String::new(), String::new()],
                    band: true,
                    strips: level,
                    span: true,
                    ..Default::default()
                },
            },
        )
    };
    let mut lines = vec![];
    match st.group_by {
        GroupBy::Nothing => {
            lines.extend(order(shown).into_iter().map(|i| report(i, 0)));
        }
        GroupBy::Scope => {
            for global in [true, false] {
                let members: Vec<usize> = shown
                    .iter()
                    .copied()
                    .filter(|&i| entries[i].scope.is_global() == global)
                    .collect();
                if members.is_empty() {
                    continue;
                }
                let (open, line) = band(
                    RowKey::ScopeBand(global),
                    if global { "Global" } else { "Project" }.into(),
                    0,
                    st,
                );
                lines.push(line);
                if open {
                    lines.extend(order(members).into_iter().map(|i| report(i, 1)));
                }
            }
        }
        GroupBy::ReportGroup => {
            let known = |g: Option<u32>| g.filter(|id| groups.iter().any(|x| x.id == *id));
            // A group is listed when something under it is shown (every
            // group is listed while nothing is filtered out).
            fn holds(
                groups: &[Group],
                entries: &[Entry],
                shown: &[usize],
                id: u32,
                depth: usize,
            ) -> bool {
                shown.iter().any(|&i| entries[i].group == Some(id))
                    || (depth < 64
                        && groups
                            .iter()
                            .filter(|g| g.parent == Some(id))
                            .any(|g| holds(groups, entries, shown, g.id, depth + 1)))
            }
            #[allow(clippy::too_many_arguments)]
            fn walk(
                groups: &[Group],
                entries: &[Entry],
                shown: &[usize],
                filtered: bool,
                parent: Option<u32>,
                level: usize,
                lines: &mut Vec<Line>,
                order: &dyn Fn(Vec<usize>) -> Vec<usize>,
                report: &dyn Fn(usize, usize) -> Line,
                band: &dyn Fn(RowKey, String, usize) -> (bool, Line),
            ) {
                if level > 64 {
                    return;
                }
                for g in groups.iter().filter(|g| g.parent == parent) {
                    if filtered && !holds(groups, entries, shown, g.id, 0) {
                        continue;
                    }
                    let (open, line) = band(RowKey::Group(g.id), g.name.clone(), level);
                    lines.push(line);
                    if !open {
                        continue;
                    }
                    walk(
                        groups,
                        entries,
                        shown,
                        filtered,
                        Some(g.id),
                        level + 1,
                        lines,
                        order,
                        report,
                        band,
                    );
                    let own: Vec<usize> = shown
                        .iter()
                        .copied()
                        .filter(|&i| entries[i].group == Some(g.id))
                        .collect();
                    lines.extend(order(own).into_iter().map(|i| report(i, level + 1)));
                }
            }
            let filtered = st.filter != Filter::All;
            walk(
                groups,
                entries,
                &shown,
                filtered,
                None,
                0,
                &mut lines,
                &order,
                &report,
                &|key, caption, level| band(key, caption, level, st),
            );
            let loose: Vec<usize> = shown
                .iter()
                .copied()
                .filter(|&i| known(entries[i].group).is_none())
                .collect();
            if !loose.is_empty() {
                let (open, line) = band(RowKey::NoGroup, "No Report Group".into(), 0, st);
                lines.push(line);
                if open {
                    lines.extend(order(loose).into_iter().map(|i| report(i, 1)));
                }
            }
        }
    }
    lines
}

/// The right-click menu and Display popup entries that choose how the view
/// is organized.
fn display_entries(st: &ReportsViewState) -> Vec<MenuEntry> {
    vec![
        MenuEntry::Submenu(
            Spec::new("group_by", "Group By"),
            vec![
                Spec::new("group_by_report_group", "Report Group")
                    .checked(st.group_by == GroupBy::ReportGroup)
                    .into(),
                Spec::new("group_by_scope", "Report Scope")
                    .checked(st.group_by == GroupBy::Scope)
                    .into(),
                Spec::new("group_by_none", "None")
                    .checked(st.group_by == GroupBy::Nothing)
                    .into(),
            ],
        ),
        MenuEntry::Submenu(
            Spec::new("sort_by", "Sort By"),
            vec![
                Spec::new("sort_name", "Report Name")
                    .checked(st.sort == SortBy::Name)
                    .into(),
                Spec::new("sort_scope", "Report Scope")
                    .checked(st.sort == SortBy::Scope)
                    .into(),
                Spec::new("sort_last_run", "Last Run Date")
                    .checked(st.sort == SortBy::LastRun)
                    .into(),
            ],
        ),
        MenuEntry::Submenu(
            Spec::new("filter_by", "Filter By"),
            [
                ("filter_all", Filter::All),
                ("filter_global", Filter::Global),
                ("filter_project", Filter::Project),
                ("filter_wizard", Filter::Wizard),
                ("filter_editor", Filter::Editor),
            ]
            .into_iter()
            .map(|(key, f)| Spec::new(key, f.label()).checked(st.filter == f).into())
            .collect(),
        ),
        MenuEntry::Separator,
        Spec::new("show_group_column", "Report Group Column")
            .checked(st.group_column_shown())
            .into(),
        MenuEntry::Separator,
        Spec::new("expand_all", "Expand All")
            .icon(Icon::ExpandAll)
            .into(),
        Spec::new("collapse_all", "Collapse All")
            .icon(Icon::CollapseAll)
            .into(),
    ]
}

/// What the right-click menu needs to know about the row under it.
struct MenuFacts {
    /// A saved report (not a built-in one): it can be modified, deleted,
    /// cut, copied, scoped and grouped.
    editable: bool,
    stored: bool,
    runnable: bool,
    global: bool,
    clipboard: bool,
}

fn row_menu(f: &MenuFacts, st: &ReportsViewState) -> Vec<MenuEntry> {
    vec![
        MenuEntry::Submenu(
            Spec::new("run", "Run").icon(Icon::Run),
            vec![
                Spec::new("run_report", "Report").enabled(f.runnable).into(),
                Spec::new("run_to_file", "Report to File...")
                    .enabled(f.runnable)
                    .into(),
                Spec::new("run_batch", "Batch...").into(),
            ],
        ),
        Spec::new("modify", "Modify...")
            .icon(Icon::Modify)
            .enabled(f.editable)
            .into(),
        MenuEntry::Separator,
        Spec::new("import", "Import...").icon(Icon::Import).into(),
        Spec::new("export", "Export...")
            .icon(Icon::Export)
            .enabled(f.stored)
            .into(),
        MenuEntry::Separator,
        Spec::new("add", "Add...").icon(Icon::Add).into(),
        Spec::new("blank", "Add Blank Report...").into(),
        Spec::new("delete", "Delete")
            .icon(Icon::Delete)
            .enabled(f.editable)
            .into(),
        MenuEntry::Separator,
        Spec::new("cut", "Cut")
            .icon(Icon::Cut)
            .enabled(f.editable)
            .into(),
        Spec::new("copy", "Copy")
            .icon(Icon::Copy)
            .enabled(f.editable)
            .into(),
        Spec::new("paste", "Paste")
            .icon(Icon::Paste)
            .enabled(f.clipboard)
            .into(),
        MenuEntry::Separator,
        MenuEntry::Submenu(
            Spec::new("scope", "Report Scope").enabled(f.editable),
            vec![
                Spec::new("scope_global", "Global")
                    .checked(f.editable && f.global)
                    .into(),
                Spec::new("scope_project", "Project")
                    .checked(f.editable && !f.global)
                    .into(),
            ],
        ),
        Spec::new("report_group", "Report Group...")
            .enabled(f.editable)
            .into(),
        MenuEntry::Separator,
        MenuEntry::Submenu(Spec::new("display", "Display Options"), display_entries(st)),
    ]
}

/// The Select Report Group dialog: the group lookup of one report.
pub struct GroupPick {
    pub which: Which,
    pub name: String,
    pub selected: Option<u32>,
}

impl App {
    /// The report selected in the view (saved or built in).
    pub(crate) fn report_selected_entry(&self) -> Option<Entry> {
        let Some(RowKey::Report(which)) = &self.views.reports.selected else {
            return None;
        };
        self.report_list().into_iter().find(|e| &e.which == which)
    }

    /// The saved report selected in the view: what the commands that
    /// change a report act on (a built-in report is not one).
    fn report_selected_saved(&self) -> Option<Entry> {
        self.report_selected_entry().filter(|e| !e.is_builtin())
    }

    /// The group a new, pasted or imported report goes in: the selected
    /// group, or the group of the selected report.
    pub(crate) fn report_target_group(&self) -> Option<u32> {
        match &self.views.reports.selected {
            Some(RowKey::Group(g)) => Some(*g),
            Some(RowKey::Report(w)) => self
                .report_list()
                .into_iter()
                .find(|e| &e.which == w)
                .and_then(|e| e.group),
            _ => None,
        }
    }

    /// A report command from the toolbar, the Edit commands or the
    /// right-click menu.
    pub(crate) fn reports_command(&mut self, key: &str) {
        let group = self.report_target_group();
        // Run on a built-in report opens its dialog, which opens the tab.
        if let (Some(Which::Builtin(b)), "run_report" | "run_to_file") =
            (self.report_selected_entry().map(|e| e.which), key)
        {
            self.open_builtin(b);
            return;
        }
        let entry = self.report_selected_saved();
        match key {
            "run_report" => {
                if let Some(e) = entry {
                    if self.can_run_entry(&e) {
                        if let Err(error) = self.run_report_in_tab(&e.which, "") {
                            self.error(error);
                        }
                    } else {
                        self.status = self.scope_refusal(&e);
                    }
                }
            }
            "run_to_file" => {
                if let Some(e) = entry {
                    if self.can_run_entry(&e) {
                        self.open_run_report(e.which.clone(), &e.name);
                    } else {
                        self.status = self.scope_refusal(&e);
                    }
                }
            }
            "run_batch" => self.open_run_batch(),
            "modify" => match entry.map(|e| e.which) {
                Some(Which::Wizard(i)) => self.modify_report(i),
                Some(Which::Stored(id)) => self.open_report_editor(id),
                Some(Which::Builtin(_)) | None => {}
            },
            "import" => self.open_report_import(group),
            "export" => {
                if let Some(Which::Stored(id)) = entry.map(|e| e.which) {
                    self.open_report_export(id);
                }
            }
            "add" | "wizard" => self.open_report_wizard_in(group),
            "blank" => match self.new_blank_report(group) {
                Ok(id) => self.open_report_editor(id),
                Err(e) => self.error(e),
            },
            "groups" => self.open_report_groups(),
            "batches" => self.open_batch_reports(),
            "delete" => {
                if let Some(e) = entry {
                    self.delete_entry(&e);
                    self.views.reports.selected = None;
                }
            }
            "cut" => {
                if let Some(e) = entry {
                    self.reports.clipboard = Some(super::rpt_entries::Clip::Move(e.which));
                    self.status = format!("Cut {}", e.name);
                }
            }
            "copy" => {
                if let Some(e) = entry {
                    self.copy_entry(&e);
                    self.status = format!("Copied {}", e.name);
                }
            }
            "paste" => match self.paste_clip(group) {
                Ok(name) => self.status = format!("Pasted {name}"),
                Err(e) => self.error(e),
            },
            "scope_global" | "scope_project" => {
                if let Some(e) = entry {
                    self.set_entry_scope(&e, key == "scope_global");
                }
            }
            "report_group" => {
                if let Some(e) = entry {
                    self.reports.pick_group = Some(GroupPick {
                        selected: e.group,
                        name: e.name,
                        which: e.which,
                    });
                }
            }
            "group_by_report_group" => self.views.reports.group_by = GroupBy::ReportGroup,
            "group_by_scope" => self.views.reports.group_by = GroupBy::Scope,
            "group_by_none" => self.views.reports.group_by = GroupBy::Nothing,
            "sort_name" => self.views.reports.sort = SortBy::Name,
            "sort_scope" => self.views.reports.sort = SortBy::Scope,
            "sort_last_run" => self.views.reports.sort = SortBy::LastRun,
            "filter_all" => self.views.reports.filter = Filter::All,
            "filter_global" => self.views.reports.filter = Filter::Global,
            "filter_project" => self.views.reports.filter = Filter::Project,
            "filter_wizard" => self.views.reports.filter = Filter::Wizard,
            "filter_editor" => self.views.reports.filter = Filter::Editor,
            "show_group_column" => {
                let order = &mut self.views.reports.columns.order;
                if order.contains(&GROUP_FIELD) {
                    order.retain(|f| *f != GROUP_FIELD);
                } else {
                    order.push(GROUP_FIELD);
                }
            }
            "expand_all" => self.views.reports.collapsed.clear(),
            "collapse_all" => {
                let groups = self.report_groups();
                let st = &mut self.views.reports;
                st.collapsed = groups.iter().map(|g| format!("g{}", g.id)).collect();
                st.collapsed.insert("none".into());
                st.collapsed.insert("scope-global".into());
                st.collapsed.insert("scope-project".into());
            }
            _ => {}
        }
    }

    pub(crate) fn reports_view(&mut self, host: &mut egui::Ui) {
        let groups_list = self.report_groups();
        let entries = self.report_list();
        let lines = build_lines(&entries, &groups_list, &self.views.reports);
        let open_id = self.open_project_id();
        let display = display_entries(&self.views.reports);
        let label = format!("Display: {}", self.views.reports.filter.label());
        let shown: Vec<Option<RowKey>> = lines.iter().map(|l| Some(l.key.clone())).collect();
        let s = &mut self.views.reports;
        let selection = s.rows.shown(&mut s.selected, &shown);
        let clipboard = self.reports.clipboard.is_some();
        let rows: Vec<Row> = lines.iter().map(|l| l.row.clone()).collect();
        let mut chosen: Option<(usize, &'static str)> = None;
        let mut clicked = None;
        let mut modifiers = egui::Modifiers::default();
        let mut secondary = false;
        let mut toggled = None;
        let mut double = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (inner, strip) = sheet::table_frame(ui, ui.max_rect());
                let bar = Rect::from_min_size(strip, Vec2::new(inner.width(), 17.));
                let header = sheet::header_button(ui, "reports", bar, &label);
                self.action("reports_display", &header);
                egui::Popup::menu(&header)
                    .style(crate::chrome::popup_menu_style)
                    .show(|ui| {
                        let mut menu = Menu::on(ui, "reports_display");
                        if let Some(key) = menu.entries(&display) {
                            chosen = Some((usize::MAX, key));
                        }
                    });
                let body = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom()), inner.max);
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let ui = &mut ui;
                let menu = |row: usize| {
                    let entry = lines.get(row).and_then(|l| match &l.key {
                        RowKey::Report(w) => entries.iter().find(|e| &e.which == w),
                        _ => None,
                    });
                    row_menu(
                        &MenuFacts {
                            editable: entry.is_some_and(|e| !e.is_builtin()),
                            stored: entry.is_some_and(|e| matches!(e.which, Which::Stored(_))),
                            runnable: entry.is_some_and(|e| e.scope.runs_on(&open_id)),
                            global: entry.is_none_or(|e| e.scope == ReportScope::Global),
                            clipboard,
                        },
                        &self.views.reports,
                    )
                };
                let open_menu = self.ctx_menu.take();
                let table = grid::Table {
                    id: "reports",
                    titles: &TITLES,
                    right: &[],
                    tree_field: 0,
                    header_height: grid::SHORT_HEADER,
                    row_lines: false,
                    band_pitch: grid::BandPitch::Styled,
                    band_caption: &[],
                    menu: Some(&menu),
                    open_menu,
                    chart_menu: None,
                };
                let mut columns = self.views.reports.columns.clone();
                let response = grid::show(
                    ui,
                    &table,
                    &mut columns,
                    &rows,
                    &selection,
                    None,
                    None,
                    &mut self.action_rects,
                );
                self.views.reports.columns = columns;
                clicked = response.clicked;
                modifiers = response.modifiers;
                secondary = response.secondary;
                toggled = response.toggled;
                double = response.double_clicked;
                if let Some(m) = response.menu {
                    chosen = Some(m);
                }
            });
        if let Some(line) = clicked.and_then(|i| lines.get(i)) {
            let s = &mut self.views.reports;
            let current = s.selected.clone();
            s.selected =
                Some(
                    s.rows
                        .click(current.as_ref(), line.key.clone(), modifiers, secondary),
                );
        }
        if let Some(line) = toggled.and_then(|i| lines.get(i)) {
            self.views.reports.toggle_band(&line.key);
        }
        if let Some(line) = double.and_then(|i| lines.get(i)) {
            match line.key {
                RowKey::Report(_) => self.reports_command("run_report"),
                _ => self.views.reports.toggle_band(&line.key),
            }
        }
        if let Some((row, key)) = chosen {
            if let Some(line) = lines.get(row) {
                self.views.reports.selected = Some(line.key.clone());
            }
            self.reports_command(key);
        }
    }

    /// The Select Report Group dialog: Select, Clear (no group) or Cancel.
    pub(crate) fn report_group_pick_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.pick_group.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 460.;
        const H: f32 = 320.;
        let list = self.report_groups();
        let rows = groups::outline_by(&list, &[]);
        let selected = d
            .selected
            .and_then(|id| rows.iter().position(|(n, _)| *n == groups::Node::Group(id)));
        #[derive(Default)]
        struct Clicks {
            select: bool,
            clear: bool,
            cancel: bool,
            help: bool,
            clicked: Option<usize>,
        }
        let (k, closed, actions) = kit::window(
            ctx,
            "pick_group",
            "Select Report Group",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 8., 8., &format!("Report: {}", d.name), true);
                let out = kit::list(
                    c,
                    "pick_group_rows",
                    (6., 28., W - 124., H - 36.),
                    None,
                    &[kit::Col {
                        title: "Report Group",
                        width: W - 126.,
                    }],
                    rows.len(),
                    selected,
                    Some(0),
                    &mut |c, i, _col, rect, s| {
                        let (node, level) = rows[i];
                        let text = match node {
                            groups::Node::Group(id) => list
                                .iter()
                                .find(|g| g.id == id)
                                .map_or(String::new(), |g| g.name.clone()),
                            groups::Node::Report(_) => String::new(),
                        };
                        kit::cell_text(c, rect, &text, s, 3. + 14. * level as f32);
                    },
                );
                k.clicked = out.clicked;
                kit::strip(c, W, H);
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button(
                            "pick_group_select",
                            "Select",
                            d.selected.is_some(),
                            false,
                        ),
                        kit::Entry::cancel("pick_group_cancel"),
                        kit::Entry::Button("pick_group_clear", "Clear", true, true),
                        kit::Entry::help("pick_group_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("pick_group_select") => k.select = true,
                    Some("pick_group_clear") => k.clear = true,
                    Some("pick_group_cancel") => k.cancel = true,
                    Some("pick_group_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        if k.help {
            self.open_help_for("pick_group");
        }
        if let Some(i) = k.clicked {
            if let Some((groups::Node::Group(id), _)) = rows.get(i) {
                d.selected = Some(*id);
            }
        }
        if k.select || k.clear {
            let group = if k.clear { None } else { d.selected };
            if let Some(e) = self
                .report_entries()
                .into_iter()
                .find(|e| e.which == d.which)
            {
                self.set_entry_group(&e, group);
            }
            return;
        }
        self.reports.pick_group = Some(d);
    }
}

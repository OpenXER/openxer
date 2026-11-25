//! Tools > Reports > Report Groups... and Batch Reports...: the report group
//! tree with the saved reports under their groups, and a batch run of
//! checked reports into one folder.
use super::doc::Output;
use super::groups::{self, Group, Node};
use super::rpt_entries::{Entry, Which};
use crate::dialog_kit as kit;
use crate::App;
use eframe::egui::{self, Align, Vec2};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Indent of one tree level in the group lists.
const INDENT: f32 = 14.;

pub struct GroupsDialog {
    pub selected: Option<Node>,
    /// A group cut or copied, waiting for Paste.
    pub cut: Option<u32>,
    pub copy: Option<u32>,
    /// The selected group's name as typed.
    pub name: String,
}

pub struct BatchDialog {
    /// Saved report indices to run.
    pub checked: BTreeSet<usize>,
    /// The checked reports in run order.
    pub order: Vec<usize>,
    /// The row picked in the run order list.
    pub order_selected: Option<usize>,
    /// The name a batch is saved under.
    pub name: String,
    /// The batch is saved as a global batch (else a project batch).
    pub global: bool,
    pub folder: String,
    /// Index into `Output::ALL`.
    pub format: usize,
    pub note: Option<String>,
    /// The saved batch last picked (0 for none).
    pub saved: usize,
}

impl App {
    /// Writes the chosen reports (wizard and definition reports) into
    /// `folder` as `format`.
    pub(crate) fn run_batch_entries(
        &self,
        picked: &[&Entry],
        folder: &std::path::Path,
        format: Output,
        notes: &str,
    ) -> Result<Vec<PathBuf>, String> {
        let wizard = self.report_definitions();
        let mut items = vec![];
        for e in picked {
            match e.which {
                Which::Wizard(i) => {
                    let Some(def) = wizard.get(i) else { continue };
                    items.push(groups::BatchItem {
                        stem: groups::file_stem(def),
                        write: Box::new(move |format, path| {
                            super::doc::write(&self.build_report(def), format, path)
                        }),
                    });
                }
                Which::Stored(id) => {
                    if format == Output::Excel {
                        return Err(format!(
                            "{} is a definition report; send it to HTML, CSV or PDF.",
                            e.name
                        ));
                    }
                    items.push(groups::BatchItem {
                        stem: groups::stem_of(&e.name),
                        write: Box::new(move |format, path| {
                            self.write_stored_as(id, format, path, notes)
                        }),
                    });
                }
                // Batches hold saved reports only.
                Which::Builtin(_) => {}
            }
        }
        groups::run_batch_items(items, folder, format)
    }

    pub(crate) fn report_groups(&self) -> Vec<Group> {
        self.store
            .preference(groups::GROUPS_KEY)
            .ok()
            .flatten()
            .map(|t| groups::groups_from_json(&t))
            .unwrap_or_default()
    }

    pub(crate) fn save_report_groups(&mut self, list: &[Group]) {
        if let Err(e) = self
            .store
            .set_preference(groups::GROUPS_KEY, &groups::groups_to_json(list))
        {
            self.error(e);
        }
    }

    pub(crate) fn open_report_groups(&mut self) {
        self.reports.groups = Some(GroupsDialog {
            selected: None,
            cut: None,
            copy: None,
            name: String::new(),
        });
    }

    pub(crate) fn open_batch_reports(&mut self) {
        let folder = self
            .store
            .preference(groups::BATCH_FOLDER_KEY)
            .ok()
            .flatten()
            .unwrap_or_else(|| super::doc::documents().display().to_string());
        self.reports.batch = Some(BatchDialog {
            saved: 0,
            checked: BTreeSet::new(),
            order: vec![],
            order_selected: None,
            name: String::new(),
            global: true,
            folder,
            format: Output::ALL
                .iter()
                .position(|o| *o == Output::Pdf)
                .unwrap_or(0),
            note: None,
        });
    }

    /// Report Groups: Close, Add (a group under the selected one), Delete
    /// (children and reports move up a level), the selected group's name,
    /// and the selected report's group.
    pub(crate) fn report_groups_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.groups.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 600.;
        const H: f32 = 400.;
        let mut list = self.report_groups();
        let saved: Vec<Entry> = self.report_entries();
        let member: Vec<Option<u32>> = saved.iter().map(|e| e.group).collect();
        let rows = groups::outline_by(&list, &member);
        let selected = d
            .selected
            .and_then(|n| rows.iter().position(|(m, _)| *m == n));
        #[derive(Default)]
        struct Clicks {
            close: bool,
            add: bool,
            delete: bool,
            cut: bool,
            copy: bool,
            paste: bool,
            /// Up, down, left (out) or right (in).
            shift: Option<&'static str>,
            help: bool,
            renamed: bool,
            regroup: Option<usize>,
            clicked: Option<usize>,
        }
        let choices: Vec<String> = std::iter::once("(none)".to_string())
            .chain(list.iter().map(|g| groups::group_path(&list, Some(g.id))))
            .collect();
        let (k, closed, actions) = kit::window(
            ctx,
            "report_groups",
            "Report Groups",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                let out = kit::list(
                    c,
                    "report_groups_rows",
                    (6., 6., W - 124., H - 82.),
                    Some("Display: Report Groups"),
                    &[
                        kit::Col {
                            title: "Report Group / Report",
                            width: 320.,
                        },
                        kit::Col {
                            title: "Subject Area",
                            width: 140.,
                        },
                    ],
                    rows.len(),
                    selected,
                    Some(0),
                    &mut |c, i, col, rect, s| {
                        let (node, level) = rows[i];
                        let text = match (node, col) {
                            (Node::Group(id), 0) => list
                                .iter()
                                .find(|g| g.id == id)
                                .map_or(String::new(), |g| g.name.clone()),
                            (Node::Report(r), 0) => saved[r].name.clone(),
                            (Node::Report(r), _) => saved[r].kind.clone(),
                            _ => String::new(),
                        };
                        let pad = 3. + if col == 0 { INDENT * level as f32 } else { 0. };
                        kit::cell_text(c, rect, &text, s, pad);
                    },
                );
                k.clicked = out.clicked;
                let y = H - 64.;
                match d.selected {
                    Some(Node::Group(_)) => {
                        kit::text(c, 6., y, "Report Group Name", true);
                        k.renamed = c
                            .edit(
                                "report_groups_name",
                                6.,
                                y + 16.,
                                W - 124.,
                                21.,
                                &mut d.name,
                                true,
                                Align::Min,
                            )
                            .changed();
                    }
                    Some(Node::Report(r)) if r < saved.len() => {
                        kit::text(c, 6., y, "Report Group", true);
                        let current = saved[r]
                            .group
                            .and_then(|g| list.iter().position(|x| x.id == g))
                            .map_or(0, |p| p + 1);
                        let mut pick = current;
                        let items: Vec<(&str, bool)> =
                            choices.iter().map(|s| (s.as_str(), true)).collect();
                        if c.combo(
                            "report_groups_group",
                            6.,
                            y + 16.,
                            320.,
                            21.,
                            &items,
                            &mut pick,
                            true,
                        ) && pick != current
                        {
                            k.regroup = Some(pick);
                        }
                    }
                    _ => {}
                }
                kit::strip(c, W, H);
                let group = matches!(d.selected, Some(Node::Group(_)));
                let paste = d.cut.is_some() || d.copy.is_some();
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::close("report_groups_close"),
                        kit::Entry::Button("report_groups_add", "Add", true, true),
                        kit::Entry::Button("report_groups_delete", "Delete", group, false),
                        kit::Entry::Button("report_groups_cut", "Cut", group, true),
                        kit::Entry::Button("report_groups_copy", "Copy", group, false),
                        kit::Entry::Button("report_groups_paste", "Paste", paste, false),
                        kit::Entry::Button("report_groups_up", "Move Up", group, true),
                        kit::Entry::Button("report_groups_down", "Move Down", group, false),
                        kit::Entry::Button("report_groups_out", "Move Left", group, false),
                        kit::Entry::Button("report_groups_in", "Move Right", group, false),
                        kit::Entry::help("report_groups_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("report_groups_close") => k.close = true,
                    Some("report_groups_add") => k.add = true,
                    Some("report_groups_delete") => k.delete = true,
                    Some("report_groups_cut") => k.cut = true,
                    Some("report_groups_copy") => k.copy = true,
                    Some("report_groups_paste") => k.paste = true,
                    Some("report_groups_up") => k.shift = Some("up"),
                    Some("report_groups_down") => k.shift = Some("down"),
                    Some("report_groups_out") => k.shift = Some("out"),
                    Some("report_groups_in") => k.shift = Some("in"),
                    Some("report_groups_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.close || closed {
            return;
        }
        if let Some(i) = k.clicked {
            d.selected = rows.get(i).map(|(n, _)| *n);
            if let Some(Node::Group(id)) = d.selected {
                d.name = list
                    .iter()
                    .find(|g| g.id == id)
                    .map_or(String::new(), |g| g.name.clone());
            }
        }
        if k.renamed {
            if let Some(Node::Group(id)) = d.selected {
                if let Some(g) = list.iter_mut().find(|g| g.id == id) {
                    g.name = d.name.clone();
                }
                self.save_report_groups(&list);
            }
        }
        if let (Some(pick), Some(Node::Report(r))) = (k.regroup, d.selected) {
            if let Some(e) = saved.get(r) {
                let group = pick.checked_sub(1).and_then(|p| list.get(p)).map(|g| g.id);
                self.set_entry_group(e, group);
            }
        }
        if k.add {
            let parent = match d.selected {
                Some(Node::Group(id)) => Some(id),
                Some(Node::Report(r)) => saved.get(r).and_then(|e| e.group),
                None => None,
            };
            let id = groups::add_group(&mut list, parent, "New Report Group");
            self.save_report_groups(&list);
            d.selected = Some(Node::Group(id));
            d.name = "New Report Group".into();
        }
        if k.delete {
            if let Some(Node::Group(id)) = d.selected {
                let top = list.iter().any(|g| g.id == id && g.parent.is_none());
                if top && member.contains(&Some(id)) {
                    // A sub-group hands its reports to its parent; a top-level
                    // group has no parent to hand them to.
                    self.error(
                        "A top-level report group that holds reports cannot be deleted. Move or delete its reports first."
                            .into(),
                    );
                } else {
                    let mut now = member.clone();
                    groups::delete_group_by(&mut list, &mut now, id);
                    self.save_report_groups(&list);
                    for (e, group) in saved.iter().zip(now) {
                        if e.group != group {
                            self.set_entry_group(e, group);
                        }
                    }
                    d.selected = None;
                }
            }
        }
        if let Some(Node::Group(id)) = d.selected {
            if k.cut {
                d.cut = Some(id);
                d.copy = None;
            }
            if k.copy {
                d.copy = Some(id);
                d.cut = None;
            }
            if let Some(key) = k.shift {
                let done = match key {
                    "up" => groups::shift(&mut list, id, true),
                    "down" => groups::shift(&mut list, id, false),
                    "out" => groups::outdent(&mut list, id),
                    _ => groups::indent(&mut list, id),
                };
                if done {
                    self.save_report_groups(&list);
                }
            }
        }
        if k.paste {
            // The pasted group goes under the selected group (or the group of
            // the selected report); with nothing selected, at the top level.
            let target = match d.selected {
                Some(Node::Group(id)) => Some(id),
                Some(Node::Report(r)) => saved.get(r).and_then(|e| e.group),
                None => None,
            };
            if let Some(id) = d.cut {
                match groups::reparent(&mut list, id, target) {
                    Ok(()) => {
                        self.save_report_groups(&list);
                        d.cut = None;
                    }
                    Err(e) => self.error(e),
                }
            } else if let Some(id) = d.copy {
                if groups::copy_subtree(&mut list, id, target).is_some() {
                    self.save_report_groups(&list);
                }
            }
        }
        if k.help {
            self.open_help_for("report_groups");
        }
        self.reports.groups = Some(d);
    }

    /// Batch Reports: the saved reports under their groups with a check
    /// box each (a group's box checks every report under it), the run
    /// order of the checked reports, the saved batches (name, scope, Save and
    /// Delete), the output folder and format, Run and Close.
    pub(crate) fn batch_reports_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.batch.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 800.;
        const H: f32 = 500.;
        const TREE_W: f32 = 360.;
        let list = self.report_groups();
        let saved: Vec<Entry> = self.report_entries();
        d.checked.retain(|&r| r < saved.len());
        // The run order keeps the checked reports in the order they were
        // checked.
        d.order.retain(|r| d.checked.contains(r));
        for r in &d.checked {
            if !d.order.contains(r) {
                d.order.push(*r);
            }
        }
        d.order_selected = d.order_selected.filter(|&i| i < d.order.len());
        let member: Vec<Option<u32>> = saved.iter().map(|e| e.group).collect();
        let rows = groups::outline_by(&list, &member);
        // Reports under each row (itself for a report row).
        let under: Vec<Vec<usize>> = rows
            .iter()
            .enumerate()
            .map(|(i, (node, level))| match node {
                Node::Report(r) => vec![*r],
                Node::Group(_) => rows[i + 1..]
                    .iter()
                    .take_while(|(_, l)| l > level)
                    .filter_map(|(n, _)| match n {
                        Node::Report(r) => Some(*r),
                        Node::Group(_) => None,
                    })
                    .collect(),
            })
            .collect();
        let checked = d.checked.clone();
        let order = d.order.clone();
        let batches = super::rpt::store::batches(self.store.connection()).unwrap_or_default();
        let batch_names: Vec<String> = std::iter::once("(none)".to_string())
            .chain(batches.iter().map(|b| b.name.clone()))
            .collect();
        let batch_items: Vec<(&str, bool)> =
            batch_names.iter().map(|n| (n.as_str(), true)).collect();
        let saved_before = d.saved;
        let on = |i: usize| !under[i].is_empty() && under[i].iter().all(|r| checked.contains(r));
        let scopes = [("Global", true), ("Project", true)];
        #[derive(Default)]
        struct Clicks {
            close: bool,
            run: bool,
            browse: bool,
            help: bool,
            all: bool,
            none: bool,
            save: bool,
            delete: bool,
            up: bool,
            down: bool,
            toggled: Option<usize>,
            order_row: Option<usize>,
        }
        let formats: Vec<(&str, bool)> = Output::ALL.iter().map(|o| (o.caption(), true)).collect();
        let (k, closed, actions) = kit::window(
            ctx,
            "batch_reports",
            "Batch Reports",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                let origin = c.origin;
                let out = kit::list(
                    c,
                    "batch_rows",
                    (6., 6., TREE_W, H - 182.),
                    Some("Display: Reports"),
                    &[
                        kit::Col {
                            title: "Report Group / Report",
                            width: 250.,
                        },
                        kit::Col {
                            title: "Scope",
                            width: 108.,
                        },
                    ],
                    rows.len(),
                    None,
                    None,
                    &mut |c, i, col, rect, s| {
                        let (node, level) = rows[i];
                        let indent = INDENT * level as f32;
                        if col == 0 {
                            kit::check_glyph(
                                c,
                                rect.left() - origin.x + 3. + indent,
                                rect.top() - origin.y + 2.,
                                on(i),
                                !under[i].is_empty(),
                            );
                        }
                        let text = match (node, col) {
                            (Node::Group(id), 0) => list
                                .iter()
                                .find(|g| g.id == id)
                                .map_or(String::new(), |g| g.name.clone()),
                            (Node::Report(r), 0) => saved[r].name.clone(),
                            (Node::Report(r), _) => saved[r].scope.caption().to_string(),
                            _ => String::new(),
                        };
                        let pad = if col == 0 { 20. + indent } else { 3. };
                        kit::cell_text(c, rect, &text, s, pad);
                    },
                );
                k.toggled = out.clicked;
                let order_out = kit::list(
                    c,
                    "batch_order",
                    (TREE_W + 12., 6., W - TREE_W - 130., H - 182.),
                    None,
                    &[kit::Col {
                        title: "Run Order",
                        width: W - TREE_W - 132.,
                    }],
                    order.len(),
                    d.order_selected,
                    Some(0),
                    &mut |c, i, _col, rect, s| {
                        let name = saved.get(order[i]).map_or("", |e| e.name.as_str());
                        kit::cell_text(c, rect, &format!("{}. {name}", i + 1), s, 3.);
                    },
                );
                k.order_row = order_out.clicked;
                let y = H - 170.;
                kit::text(c, 6., y, "Output folder", true);
                c.edit(
                    "batch_reports_folder",
                    6.,
                    y + 16.,
                    W - 124.,
                    21.,
                    &mut d.folder,
                    true,
                    Align::Min,
                );
                let field = c.r(6., y + 16., W - 124., 21.);
                k.browse = c.ellipsis("batch_reports_browse", field, true).clicked();
                kit::text(c, 6., y + 46., "Format", true);
                c.combo(
                    "batch_reports_format",
                    6.,
                    y + 62.,
                    200.,
                    21.,
                    &formats,
                    &mut d.format,
                    true,
                );
                kit::text(c, 220., y + 46., "Saved batch", true);
                c.combo(
                    "batch_reports_saved",
                    220.,
                    y + 62.,
                    240.,
                    21.,
                    &batch_items,
                    &mut d.saved,
                    !batches.is_empty(),
                );
                kit::text(c, 6., y + 92., "Batch name", true);
                c.edit(
                    "batch_reports_name",
                    6.,
                    y + 108.,
                    300.,
                    21.,
                    &mut d.name,
                    true,
                    Align::Min,
                );
                kit::text(c, 320., y + 92., "Report scope", true);
                let mut pick = usize::from(!d.global);
                if c.combo(
                    "batch_reports_scope",
                    320.,
                    y + 108.,
                    140.,
                    21.,
                    &scopes,
                    &mut pick,
                    true,
                ) {
                    d.global = pick == 0;
                }
                if let Some(note) = &d.note {
                    kit::text(c, 6., y + 140., note, true);
                }
                kit::strip(c, W, H);
                let ready = !d.checked.is_empty() && !d.folder.trim().is_empty();
                let at = d.order_selected;
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::close("batch_reports_close"),
                        kit::Entry::Button("batch_reports_run", "Run", ready, true),
                        kit::Entry::Button(
                            "batch_reports_all",
                            "Select All",
                            !saved.is_empty(),
                            true,
                        ),
                        kit::Entry::Button(
                            "batch_reports_none",
                            "Clear All",
                            !d.checked.is_empty(),
                            false,
                        ),
                        kit::Entry::Button(
                            "batch_reports_up",
                            "Move Up",
                            at.is_some_and(|i| i > 0),
                            true,
                        ),
                        kit::Entry::Button(
                            "batch_reports_down",
                            "Move Down",
                            at.is_some_and(|i| i + 1 < order.len()),
                            false,
                        ),
                        kit::Entry::Button(
                            "batch_reports_save",
                            "Save Batch",
                            !d.checked.is_empty() && !d.name.trim().is_empty(),
                            true,
                        ),
                        kit::Entry::Button(
                            "batch_reports_delete",
                            "Delete Batch",
                            d.saved > 0,
                            false,
                        ),
                        kit::Entry::help("batch_reports_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("batch_reports_close") => k.close = true,
                    Some("batch_reports_run") => k.run = true,
                    Some("batch_reports_all") => k.all = true,
                    Some("batch_reports_none") => k.none = true,
                    Some("batch_reports_up") => k.up = true,
                    Some("batch_reports_down") => k.down = true,
                    Some("batch_reports_save") => k.save = true,
                    Some("batch_reports_delete") => k.delete = true,
                    Some("batch_reports_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.close || closed {
            return;
        }
        if d.saved != saved_before {
            // A saved batch checks its reports, in its order.
            d.checked.clear();
            d.order.clear();
            if let Some(b) = d.saved.checked_sub(1).and_then(|n| batches.get(n)) {
                for id in &b.reports {
                    if let Some(at) = saved.iter().position(|e| e.which == Which::Stored(*id)) {
                        d.checked.insert(at);
                        d.order.push(at);
                    }
                }
                d.name = b.name.clone();
                d.global = b.scope.is_global();
            }
        }
        if let Some(i) = k.toggled {
            if on(i) {
                for r in &under[i] {
                    d.checked.remove(r);
                }
            } else {
                for r in &under[i] {
                    if d.checked.insert(*r) {
                        d.order.push(*r);
                    }
                }
            }
        }
        if let Some(i) = k.order_row {
            d.order_selected = Some(i);
        }
        if let Some(i) = d.order_selected {
            if k.up && i > 0 {
                d.order.swap(i, i - 1);
                d.order_selected = Some(i - 1);
            }
            if k.down && i + 1 < d.order.len() {
                d.order.swap(i, i + 1);
                d.order_selected = Some(i + 1);
            }
        }
        if k.all {
            d.checked = (0..saved.len()).collect();
            d.order = (0..saved.len()).collect();
        }
        if k.none {
            d.checked.clear();
            d.order.clear();
        }
        if k.browse {
            if let Some(folder) = rfd::FileDialog::new()
                .set_directory(d.folder.trim())
                .pick_folder()
            {
                d.folder = folder.display().to_string();
            }
        }
        if k.help {
            self.open_help_for("batch_reports");
        }
        if k.save {
            match self.save_batch_from(&d, &saved) {
                Ok(name) => {
                    d.note = Some(format!("Saved batch {name}"));
                    let now =
                        super::rpt::store::batches(self.store.connection()).unwrap_or_default();
                    d.saved = now.iter().position(|b| b.name == name).map_or(0, |p| p + 1);
                }
                Err(e) => self.error(e),
            }
        }
        if k.delete {
            if let Some(b) = d.saved.checked_sub(1).and_then(|n| batches.get(n)) {
                if let Err(e) = super::rpt::store::delete_batch(self.store.connection(), b.id) {
                    self.error(e);
                } else {
                    d.note = Some(format!("Deleted batch {}", b.name));
                    d.saved = 0;
                }
            }
        }
        if k.run {
            let format = Output::ALL[d.format.min(Output::ALL.len() - 1)];
            let folder = PathBuf::from(d.folder.trim());
            let picked: Vec<Entry> = d
                .order
                .iter()
                .filter_map(|&r| saved.get(r).cloned())
                .collect();
            let refused = picked.iter().find(|e| !self.can_run_entry(e));
            if let Some(e) = refused {
                let text = self.scope_refusal(e);
                self.error(text);
            } else {
                let refs: Vec<&Entry> = picked.iter().collect();
                match self.run_batch_entries(&refs, &folder, format, "") {
                    Ok(files) => {
                        for e in &picked {
                            self.mark_run(&e.which);
                        }
                        let text = format!(
                            "{} report{} written to {}",
                            files.len(),
                            if files.len() == 1 { "" } else { "s" },
                            folder.display()
                        );
                        self.status = text.clone();
                        d.note = Some(text);
                        if let Err(e) = self
                            .store
                            .set_preference(groups::BATCH_FOLDER_KEY, d.folder.trim())
                        {
                            self.error(e);
                        }
                    }
                    Err(e) => self.error(e),
                }
            }
        }
        self.reports.batch = Some(d);
    }

    /// Saves the dialog's checked reports, in run order, as a batch. A global
    /// batch takes only global reports and a project batch only reports of
    /// the open project; only definition reports can be saved in a batch.
    fn save_batch_from(&mut self, d: &BatchDialog, saved: &[Entry]) -> Result<String, String> {
        use super::rpt::store::ReportScope;
        let scope = if d.global {
            ReportScope::Global
        } else {
            ReportScope::Project(self.open_project_id())
        };
        let mut ids = vec![];
        for &r in &d.order {
            let Some(e) = saved.get(r) else { continue };
            let Which::Stored(id) = e.which else {
                return Err(format!(
                    "{} was made with the Report Wizard; only definition reports can be saved in a batch.",
                    e.name
                ));
            };
            let fits = match &scope {
                ReportScope::Global => e.scope.is_global(),
                project => &e.scope == project,
            };
            if !fits {
                return Err(format!(
                    "{} is a {} report; a {} batch takes {} reports.",
                    e.name,
                    e.scope.caption().to_lowercase(),
                    scope.caption().to_lowercase(),
                    scope.caption().to_lowercase(),
                ));
            }
            ids.push(id);
        }
        let name = d.name.trim().to_string();
        super::rpt::store::save_batch_scoped(self.store.connection(), &name, &scope, &ids)?;
        Ok(name)
    }
}

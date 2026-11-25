//! The activity table's context menu. One menu serves a right-click on a
//! row, a group band, the empty area under the rows and a column header
//! (which has no menu of its own): 21 commands and 6 separators in a fixed
//! order, drawn in the native popup style. Commands this build
//! does not have are shown disabled with the usual hint.
use super::*;
use crate::context_menu::{Caps, ContextMenu, Menu};
use chrome::{Icon, Item};
use details::assignments::ResourceAssign;

/// The Assign submenu: the Edit menu's items, each opening its Activity
/// Details tab. Action names are `<prefix>_assign_<key>`.
const ASSIGN: [(&str, &str, Icon, Command); 6] = [
    (
        "resources",
        "&Resources...",
        Icon::AssignResource,
        Command::AssignResource(ResourceAssign::Resources),
    ),
    (
        "by_role",
        "Resources by Ro&le...",
        Icon::AssignByRole,
        Command::AssignResource(ResourceAssign::ByRole),
    ),
    (
        "roles",
        "R&oles...",
        Icon::AssignRole,
        Command::AssignResource(ResourceAssign::Roles),
    ),
    (
        "codes",
        "Activity &Codes...",
        Icon::AssignCode,
        Command::Assign(DetailTab::Codes),
    ),
    (
        "predecessors",
        "&Predecessors...",
        Icon::Predecessors,
        Command::Assign(DetailTab::Predecessors),
    ),
    (
        "successors",
        "&Successors...",
        Icon::Successors,
        Command::Assign(DetailTab::Successors),
    ),
];

impl App {
    /// One context menu entry that queues `command`; `<prefix>_<key>` names
    /// its rectangle for tests.
    #[allow(clippy::too_many_arguments)]
    fn table_item(
        &mut self,
        ui: &mut egui::Ui,
        prefix: &str,
        key: &str,
        item: Item,
        command: Command,
    ) {
        if self.item(ui, &format!("{prefix}_{key}"), item) {
            self.editing.pending = Some(command);
        }
    }

    /// The Assign submenu items, shared by the Edit menu (`menu`) and the
    /// table context menu.
    pub(crate) fn assign_items(&mut self, ui: &mut egui::Ui, prefix: &str) {
        for (key, label, icon, command) in ASSIGN {
            self.table_item(
                ui,
                prefix,
                &format!("assign_{key}"),
                Item::new(label).icon(icon),
                command,
            );
        }
    }

    /// What the activity table can do right now: the capability set the
    /// table menu reads on every open.
    pub(crate) fn table_caps(&self) -> Caps {
        Caps {
            can_edit: true,
            can_cut: true,
            can_paste: self.editing.clipboard.is_some(),
            selection_count: self.selected_in_order().len(),
            view_has_gantt: self.panes.top != panes::TopView::Table,
            has_rows: !self.table_activities().is_empty(),
        }
    }

    /// Opens the table context menu on a right-click of `response`. A
    /// click on a disabled item or a separator leaves it open.
    pub(crate) fn table_menu_popup(&mut self, response: &egui::Response, prefix: &str) {
        self.table_menu_popup_at(response, prefix, None);
    }

    /// The same, opened at `at` when the keyboard asked for it.
    pub(crate) fn table_menu_popup_at(
        &mut self,
        response: &egui::Response,
        prefix: &str,
        at: Option<Pos2>,
    ) {
        // The capabilities are worked out only while the menu is open: this
        // runs for every header and every row under the pointer each frame.
        ContextMenu::new(prefix)
            .min_width(199.)
            .open_at(at)
            .show(response, self, |app, menu| {
                menu.set_caps(app.table_caps());
                app.table_context_menu(menu)
            });
    }

    /// The table context menu. The prefix of the menu names the rectangles:
    /// `ctx` for a row and `header_menu` for a column header. Selection
    /// commands read the capability set, so a right-click on empty space
    /// shows them disabled and the table-wide ones enabled.
    pub(crate) fn table_context_menu(&mut self, m: &mut Menu) {
        let caps = m.caps();
        let any = caps.has_selection();
        let many = caps.several();
        let mut run = None;
        fn put(run: &mut Option<Command>, clicked: bool, command: Command) {
            if clicked {
                *run = Some(command);
            }
        }
        put(
            &mut run,
            m.item(
                "activity_details",
                Item::new("Activity Details").icon(Icon::Details),
            ),
            Command::ActivityDetails,
        );
        m.separator();
        for (key, label, shortcut, icon, enabled, command) in [
            (
                "cut",
                "Cut",
                "Ctrl+X",
                Icon::Cut,
                caps.cut_enabled(),
                Command::Cut,
            ),
            (
                "copy",
                "Copy",
                "Ctrl+C",
                Icon::Copy,
                caps.copy_enabled(),
                Command::Copy,
            ),
            (
                "paste",
                "Paste",
                "Ctrl+V",
                Icon::Paste,
                caps.paste_enabled(),
                Command::Paste,
            ),
            (
                "fill_down",
                "Fill Down",
                "Ctrl+E",
                Icon::FillDown,
                caps.can_edit && many,
                Command::FillDown,
            ),
        ] {
            let item = Item::new(label).key(shortcut).icon(icon).enabled(enabled);
            put(&mut run, m.item(key, item), command);
        }
        m.separator();
        let item = Item::new("Add").key("Ins").icon(Icon::Add);
        put(
            &mut run,
            m.item("add", item.enabled(caps.can_edit)),
            Command::Add,
        );
        let item = Item::new("Delete")
            .key("Del")
            .icon(Icon::Delete)
            .enabled(caps.edit_selection_enabled());
        put(&mut run, m.item("delete", item), Command::Delete);
        // One activity at a time, as in the reference: several rows grey it.
        let item = Item::new("Dissolve")
            .enabled(caps.edit_selection_enabled() && !many && self.can_dissolve_selection());
        put(&mut run, m.item("dissolve", item), Command::Dissolve);
        let item = Item::new("Renumber Activity IDs").enabled(caps.edit_selection_enabled());
        put(&mut run, m.item("renumber", item), Command::Renumber);
        let item = Item::new("Export to Excel");
        put(&mut run, m.item("export_excel", item), Command::ExportExcel);
        m.separator();
        // The submenu always opens; its items need an activity selected
        // (a band or the empty area greys them).
        m.submenu("assign", Item::new("Assign"), |m| {
            for (key, label, icon, command) in ASSIGN {
                let item = Item::new(label).icon(icon).enabled(any);
                if m.item(&format!("assign_{key}"), item) {
                    run = Some(command);
                }
            }
        });
        let item = Item::new("Link Activities")
            .icon(Icon::Link)
            .enabled(caps.can_edit && many);
        put(&mut run, m.item("link", item), Command::Link);
        let item = Item::new("Unlink Activities")
            .icon(Icon::Unlink)
            .enabled(caps.can_edit && many);
        put(&mut run, m.item("unlink", item), Command::Unlink);
        m.separator();
        let item = Item::new("Columns...").icon(Icon::Columns);
        put(&mut run, m.item("columns", item), Command::Columns);
        let item = Item::new("Line Numbers")
            .icon(Icon::LineNumbers)
            .checked(self.grid.layout.table.line_numbers);
        put(&mut run, m.item("line_numbers", item), Command::LineNumbers);
        let item = Item::new("Table Font and Row...").icon(Icon::TableFont);
        put(&mut run, m.item("table_font", item), Command::TableFont);
        m.separator();
        let item = Item::new("Filters...").icon(Icon::Filter);
        put(&mut run, m.item("filters", item), Command::Filters);
        let item = Item::new("Group and Sort...").icon(Icon::GroupSort);
        put(&mut run, m.item("group_sort", item), Command::GroupSort);
        m.separator();
        let item = Item::new("Expand All")
            .key("Ctrl+Num +")
            .icon(Icon::ExpandAll)
            .enabled(!self.collapsed.is_empty());
        put(&mut run, m.item("expand_all", item), Command::ExpandAll);
        let item = Item::new("Collapse All")
            .key("Ctrl+Num -")
            .icon(Icon::CollapseAll);
        put(&mut run, m.item("collapse_all", item), Command::CollapseAll);
        let item = Item::new("Collapse To...").icon(Icon::CollapseTo);
        put(&mut run, m.item("collapse_to", item), Command::CollapseTo);
        if let Some(command) = run {
            self.editing.pending = Some(command);
        }
    }

    /// The menu of a group band: the table menu itself, as in the
    /// reference, read with the band selected (no activity), so the
    /// selection commands are greyed. Rectangles are `band_<key>`.
    pub(crate) fn band_menu_popup_at(&mut self, response: &egui::Response, at: Option<Pos2>) {
        self.table_menu_popup_at(response, "band", at);
    }

    /// Assign > Resources, Resources by Role and Roles open their pickers at
    /// once on the Resources tab, for every selected activity.
    pub(crate) fn open_assign_resource(&mut self, kind: ResourceAssign) {
        self.show_detail_tab(DetailTab::Resources);
        if self
            .selected
            .is_none_or(|i| i >= self.project.model.activities.len())
        {
            self.status = "Select an activity first.".into();
            return;
        }
        let has = |table: &str| {
            self.project
                .document
                .table(table)
                .is_some_and(|t| !t.rows.is_empty())
        };
        // Resources can be added from the Assign Resources window itself;
        // roles come from the Roles dictionary.
        if kind != ResourceAssign::Resources && !has("ROLES") {
            self.status = "This project file has no roles.".into();
            return;
        }
        self.detail.new_resource = None;
        self.detail.assign = Some(match kind {
            ResourceAssign::Resources => details::assignments::AssignPicker::Resource {
                query: String::new(),
                selected: None,
            },
            ResourceAssign::Roles => details::assignments::AssignPicker::Role { selected: None },
            ResourceAssign::ByRole => {
                details::assignments::AssignPicker::PickRole { selected: None }
            }
        });
    }

    /// Assign > Predecessors, Successors, Activity Codes and (from the
    /// network menu) Work Products open their assign pickers at once (on
    /// their Activity Details tab); each applies to every selected activity.
    pub(crate) fn open_assign(&mut self, tab: DetailTab) {
        self.show_detail_tab(tab);
        if self
            .selected
            .is_none_or(|i| i >= self.project.model.activities.len())
        {
            self.status = "Select an activity first.".into();
            return;
        }
        self.detail.picker = match tab {
            DetailTab::Predecessors | DetailTab::Successors => {
                Some(details::Picker::Relationship {
                    predecessor: tab == DetailTab::Predecessors,
                    query: String::new(),
                    selected: None,
                    kind: LinkType::FS,
                    lag: 0.,
                })
            }
            DetailTab::Codes => {
                // No values yet: the Activity Codes dictionary opens first.
                self.assign_activity_codes();
                self.detail.picker.take()
            }
            DetailTab::Documents => self.document_picker(),
            _ => self.detail.picker.take(),
        };
    }

    /// Link Activities: finish to start ties between the selected
    /// activities in table order, as one undo step.
    pub(crate) fn link_selected(&mut self, time: f64) {
        let rows = self.selected_in_order();
        if rows.len() < 2 {
            self.status = "Select two or more activities to link.".into();
            return;
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        match links::link_in_order(&mut self.project.model, &rows) {
            Ok(out) => {
                if out.added > 0 {
                    self.edit_step(before, before_revision, time);
                } else {
                    self.project = before;
                }
                let mut note = String::new();
                if out.existing > 0 {
                    note += &format!(", {} already linked", out.existing);
                }
                if out.circular > 0 {
                    note += &format!(", {} skipped to avoid a loop", out.circular);
                }
                self.status = match out.added {
                    1 => format!("1 relationship added{note}"),
                    n => format!("{n} relationships added{note}"),
                };
            }
            Err(e) => {
                self.project = before;
                self.error(format!("Cannot link the activities: {e}"));
            }
        }
    }

    /// Unlink Activities: remove every tie among the selected activities,
    /// as one undo step.
    pub(crate) fn unlink_selected(&mut self, time: f64) {
        let rows = self.selected_in_order();
        if rows.len() < 2 {
            self.status = "Select two or more activities to unlink.".into();
            return;
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        match links::unlink_among(&mut self.project.model, &rows) {
            Ok(0) => {
                self.project = before;
                self.status = "The selected activities are not linked to each other.".into();
            }
            Ok(n) => {
                self.edit_step(before, before_revision, time);
                self.status = match n {
                    1 => "1 relationship removed".into(),
                    n => format!("{n} relationships removed"),
                };
            }
            Err(e) => {
                self.project = before;
                self.error(format!("Cannot unlink the activities: {e}"));
            }
        }
    }

    /// Export to Excel. Tests never open the native save dialog.
    pub(crate) fn table_export_excel(&mut self) {
        #[cfg(not(test))]
        self.export_excel();
    }
}

#[cfg(test)]
#[path = "table_menu_tests.rs"]
mod tests;

//! Main menu bar, its dropdowns and the docked toolbar rows. Item order,
//! separators and shortcut keys are fixed per menu; commands this build does
//! not have yet are shown disabled. Each menu and each toolbar is its own method so packages
//! can add their commands without touching the others.
use super::*;
use chrome::{menu_separator as separator, Icon, Item};

/// Height of the menu bar row including its bottom line.
pub const MENU_ROW: f32 = 25.;
/// Height of one toolbar row including its bottom line.
pub const TOOLBAR_ROW: f32 = 28.;

/// Toolbars of the first and second top rows, in display order.
pub const TOP_ROW_BARS: [&str; 6] = [
    "Print",
    "Top Layout",
    "Bottom Layout",
    "Layout",
    "Tools",
    "Display",
];
/// The second top row: Dictionaries, then the toolbars that start hidden,
/// when they are switched on.
pub const SECOND_ROW_BARS: [&str; 7] = [
    "Dictionaries",
    "Activity Critical Path",
    "Action",
    "Administrator",
    "Find",
    "Reports",
    "Check Schedule",
];
/// Toolbars that start hidden (unchecked in View > Toolbars) until the user
/// switches them on, as the reference's do; Reset All Toolbars hides them
/// again.
pub const OPTIONAL_TOOLBARS: [&str; 6] = [
    "Activity Critical Path",
    "Action",
    "Administrator",
    "Find",
    "Reports",
    "Check Schedule",
];
/// Every toolbar this build draws (top rows and side docks).
pub const BUILT_TOOLBARS: [&str; 19] = [
    "Print",
    "Top Layout",
    "Bottom Layout",
    "Layout",
    "Tools",
    "Display",
    "Dictionaries",
    "Standard",
    "Enterprise",
    "Project",
    "Edit",
    "Assign",
    "Move",
    "Activity Critical Path",
    "Action",
    "Administrator",
    "Find",
    "Reports",
    "Check Schedule",
];

/// The toolbars in the order View > Toolbars lists them.
pub const TOOLBAR_LIST: [&str; 19] = [
    "Project",
    "Assign",
    "Edit",
    "Layout",
    "Enterprise",
    "Print",
    "Action",
    "Administrator",
    "Bottom Layout",
    "Dictionaries",
    "Display",
    "Find",
    "Reports",
    "Standard",
    "Tools",
    "Top Layout",
    "Move",
    "Activity Critical Path",
    "Check Schedule",
];

impl App {
    /// A plain toolbar button that records its action rect; true on click.
    pub(super) fn tool(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        icon: chrome::Icon,
        tip: &str,
        enabled: bool,
    ) -> bool {
        self.tool_ex(ui, name, icon, tip, enabled, false, false)
    }

    /// A toolbar button that can show a checked state or a dropdown arrow.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn tool_ex(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        icon: chrome::Icon,
        tip: &str,
        enabled: bool,
        checked: bool,
        dropdown: bool,
    ) -> bool {
        if !self.button_visible(name) {
            return false;
        }
        let response = chrome::tool_button(ui, icon, tip, enabled, checked, dropdown);
        #[cfg(test)]
        self.views
            .bars
            .rt
            .drawn_icons
            .insert(name.to_string(), icon);
        self.note_button_width(name, response.rect.width());
        self.action(name, &response);
        enabled && response.clicked()
    }

    /// A dropdown item that records its action rect and closes the menu when
    /// clicked; true on click.
    pub(super) fn item(&mut self, ui: &mut egui::Ui, action: &str, item: Item) -> bool {
        let response = chrome::menu_item(ui, item);
        if !action.is_empty() {
            self.action(action, &response);
        }
        let clicked = item.enabled && response.clicked();
        if clicked {
            ui.close();
        }
        clicked
    }

    /// A Gantt chart is on screen: the top Gantt view, or the Gantt chart
    /// shown at the bottom. Bars... needs one.
    pub(crate) fn gantt_on_screen(&self) -> bool {
        self.panes.top == panes::TopView::Gantt || self.bottom_showing(panes::BottomView::Gantt)
    }

    /// An activity table is on screen: the top Table or Gantt view, or the
    /// Activity Table shown at the bottom. Columns... needs one.
    pub(crate) fn table_on_screen(&self) -> bool {
        matches!(
            self.panes.top,
            panes::TopView::Gantt | panes::TopView::Table
        ) || self.bottom_showing(panes::BottomView::ActivityTable)
    }

    pub(super) fn toolbar_visible(&self, name: &str) -> bool {
        if OPTIONAL_TOOLBARS.contains(&name) {
            self.views.shown_toolbars.contains(name)
        } else {
            !self.views.hidden_toolbars.contains(name)
        }
    }

    /// The top dock: the menu bar and the toolbar rows, each white with a
    /// #A0A0A0 line under it, as the layout places them. Runs first each
    /// frame, so it also starts the bars' per-frame state.
    pub(super) fn menu(&mut self, ctx: &egui::Context) {
        self.begin_bars(ctx);
        self.edge_dock(ctx, crate::dockbars::Side::Top);
    }

    /// The titles of the menu bar, in the dropdown direction `style` gives.
    pub(crate) fn menu_bar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        style: chrome::TitleStyle,
    ) {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let titles = [
            chrome::menu_bar_title(ui, "&File", style, |ui| self.file_menu(ui, ctx)).0,
            chrome::menu_bar_title(ui, "&Edit", style, |ui| self.edit_menu(ui, ctx)).0,
            chrome::menu_bar_title(ui, "&View", style, |ui| self.view_menu(ui)).0,
            chrome::menu_bar_title(ui, "&Project", style, |ui| self.project_menu(ui)).0,
            chrome::menu_bar_title(ui, "E&nterprise", style, |ui| self.enterprise_menu(ui)).0,
            chrome::menu_bar_title(ui, "&Tools", style, |ui| self.tools_menu(ui)).0,
            chrome::menu_bar_title(ui, "A&I Agents", style, |ui| self.ai_agents_menu(ui)).0,
            chrome::menu_bar_title(ui, "&Admin", style, |ui| self.admin_menu(ui)).0,
            chrome::menu_bar_title(ui, "&Help", style, |ui| self.help_menu(ui)).0,
        ];
        for (name, response) in [
            "file",
            "edit",
            "view",
            "project",
            "enterprise",
            "tools",
            "ai_agents",
            "admin",
            "help",
        ]
        .iter()
        .zip(&titles)
        {
            self.action(&format!("menu_bar_{name}"), response);
        }
    }

    pub(super) fn refresh_data(&mut self) {
        self.date_edits.clear();
        self.reorganize_now();
        self.status = "View refreshed".into();
    }

    fn file_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.item(
            ui,
            "menu_new",
            Item::new("&New...").key("Ctrl+N").icon(Icon::New),
        ) {
            self.open_new_project();
        }
        if self.item(
            ui,
            "menu_open",
            Item::new("&Open...").key("Ctrl+O").icon(Icon::Open),
        ) {
            self.open_project_dialog();
        }
        if self.item(
            ui,
            "menu_close_all",
            Item::new("&Close All")
                .key("Ctrl+W")
                .icon(Icon::CloseAll)
                .enabled(self.open_set.multi()),
        ) {
            self.close_all_projects();
        }
        separator(ui);
        let printable = self.can_print();
        let page_setup = Item::new("Page Set&up...")
            .icon(Icon::PageSetup)
            .enabled(printable);
        if self.item(ui, "menu_page_setup", page_setup) {
            self.open_page_setup();
        }
        let preview = Item::new("Print Pre&view")
            .icon(Icon::PrintPreview)
            .enabled(printable);
        if self.item(ui, "menu_print_preview", preview) {
            self.show_print_preview();
        }
        let print = Item::new("&Print...")
            .key("Ctrl+P")
            .icon(Icon::Print)
            .enabled(printable || self.current_report_tab().is_some());
        if self.item(ui, "menu_print", print) {
            self.print_command();
        }
        separator(ui);
        if self.item(
            ui,
            "file_import",
            Item::new("&Import...").icon(Icon::Import),
        ) {
            self.open_import_wizard();
        }
        if self.item(
            ui,
            "file_export",
            Item::new("&Export...").icon(Icon::Export),
        ) {
            self.open_export_wizard();
        }
        separator(ui);
        // Save writes every open project with changes (F10 also works).
        let save = Item::new("&Save")
            .key("Ctrl+S")
            .icon(Icon::Save)
            .enabled(self.any_dirty());
        if self.item(ui, "save", save) {
            self.save();
        }
        if self.item(
            ui,
            "menu_refresh",
            Item::new("&Refresh View").key("F5").icon(Icon::Refresh),
        ) {
            self.refresh_data();
        }
        separator(ui);
        let recent = Item::new("Recent Pro&jects").enabled(!self.saved_projects.is_empty());
        chrome::submenu(ui, recent, |ui| {
            let recent: Vec<_> = self.saved_projects.iter().take(10).cloned().collect();
            for (number, (id, name)) in recent.into_iter().enumerate() {
                let label = format!("&{} {name}", number + 1);
                if self.item(ui, &format!("menu_recent_{id}"), Item::new(&label)) {
                    match self.store.load(id) {
                        Ok(p) => {
                            self.load(p);
                            self.show_view(views::MainView::Activities);
                        }
                        Err(e) => self.error(e),
                    }
                }
            }
        });
        separator(ui);
        if self.item(ui, "menu_exit", Item::new("E&xit")) {
            self.exit(ctx);
        }
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (undo, redo) = self.history.counts();
        let activities = self.views.main.is_activities();
        if self.item(
            ui,
            "undo",
            Item::new("&Undo")
                .key("Ctrl+Z")
                .icon(Icon::Undo)
                .enabled(undo > 0),
        ) {
            self.history_move(ctx, false);
        }
        // Redo is an OpenXER addition next to Undo.
        if self.item(
            ui,
            "redo",
            Item::new("&Redo")
                .key("Ctrl+Y")
                .icon(Icon::Redo)
                .enabled(redo > 0),
        ) {
            self.history_move(ctx, true);
        }
        separator(ui);
        let any = activities && self.selected.is_some();
        let many = activities && self.selected_in_order().len() > 1;
        // Cut, Copy, Paste, Add and Delete: the shown view's commands, the
        // same ones its right-click menu, the Edit toolbar and the keys run
        // (`ViewEdit`; in the Activities view the table's `editing::Command`).
        let view = |app: &Self, edit| app.view_edit_enabled(edit) == Some(true);
        let time = ctx.input(|i| i.time);
        for (action, caption, icon, edit) in [
            ("edit_cut", "Cu&t", Icon::Cut, views::ViewEdit::Cut),
            ("edit_copy", "&Copy", Icon::Copy, views::ViewEdit::Copy),
            ("edit_paste", "&Paste", Icon::Paste, views::ViewEdit::Paste),
        ] {
            let item = Item::new(caption).icon(icon).enabled(view(self, edit));
            if self.item(ui, action, item) {
                self.view_edit(edit, time);
            }
        }
        separator(ui);
        for (action, caption, icon, edit) in [
            ("menu_add", "A&dd", Icon::Add, views::ViewEdit::Add),
            (
                "edit_delete",
                "De&lete",
                Icon::Delete,
                views::ViewEdit::Delete,
            ),
        ] {
            let item = Item::new(caption).icon(icon).enabled(view(self, edit));
            if self.item(ui, action, item) {
                self.view_edit(edit, time);
            }
        }
        self.menu_item(
            ui,
            "edit_dissolve",
            "Diss&olve",
            "",
            activities && self.can_dissolve_selection(),
            editing::Command::Dissolve,
        );
        self.menu_item(
            ui,
            "edit_renumber",
            "Renumber Activity IDs",
            "",
            any,
            editing::Command::Renumber,
        );
        separator(ui);
        chrome::submenu(ui, Item::new("Ass&ign").enabled(activities), |ui| {
            self.assign_items(ui, "menu")
        });
        self.menu_item_icon(
            ui,
            "edit_link",
            "Lin&k Activities",
            "",
            many,
            editing::Command::Link,
            Some(Icon::Link),
        );
        self.menu_item_icon(
            ui,
            "edit_unlink",
            "Unlink Acti&vities",
            "",
            many,
            editing::Command::Unlink,
            Some(Icon::Unlink),
        );
        separator(ui);
        self.menu_item_icon(
            ui,
            "edit_fill_down",
            "Fill Do&wn",
            "Ctrl+E",
            many || self.view_fill_ready(),
            editing::Command::FillDown,
            Some(Icon::FillDown),
        );
        self.menu_item(
            ui,
            "edit_select_all",
            "Select &All",
            "Ctrl+A",
            activities || self.grid_view_keys_ready(),
            editing::Command::SelectAll,
        );
        separator(ui);
        if self.item(
            ui,
            "menu_find",
            Item::new("&Find...").key("Ctrl+F").icon(Icon::Find),
        ) {
            self.open_find(false);
        }
        let next = self.find_ready();
        // The activity table, or a view table with the cell commands.
        let findable = activities || self.view_find_ready();
        if self.item(
            ui,
            "menu_find_next",
            Item::new("Find Ne&xt").key("F3").enabled(findable && next),
        ) {
            self.run_find_next();
        }
        if self.item(
            ui,
            "menu_replace",
            Item::new("R&eplace...")
                .key("Ctrl+R")
                .icon(Icon::Replace)
                .enabled(findable),
        ) {
            self.open_find(true);
        }
        separator(ui);
        self.spell_menu_item(ui);
        separator(ui);
        if self.item(
            ui,
            "menu_user_preferences",
            Item::new("User Prefere&nces...").icon(Icon::Preferences),
        ) {
            self.open_user_preferences();
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        let activities = self.views.main.is_activities();
        chrome::submenu(ui, Item::new("Lay&out").enabled(activities), |ui| {
            if self.item(ui, "menu_new_layout", Item::new("&New Layout...")) {
                self.open_new_layout();
            }
            self.layout_file_items(ui);
        });
        separator(ui);
        chrome::submenu(ui, Item::new("Sho&w on Top").enabled(activities), |ui| {
            self.top_menu_items(ui);
        });
        // The Resources window shows or hides its details under the
        // resource table.
        let resources = self.views.main == views::MainView::Resources;
        let bottom = Item::new("S&how on Bottom").enabled(activities || resources);
        chrome::submenu(ui, bottom, |ui| {
            if resources {
                self.resources_bottom_items(ui);
            } else {
                self.show_on_bottom_items(ui);
            }
        });
        if self.item(
            ui,
            "menu_bottom_layout_options",
            Item::new("Bottom Layout Options...").enabled(activities),
        ) {
            self.open_bottom_layout_options();
        }
        separator(ui);
        if self.item(
            ui,
            "view_bars",
            Item::new("Bars...")
                .icon(Icon::Bars)
                .enabled(activities && self.gantt_on_screen()),
        ) {
            self.open_bars_dialog();
        }
        chrome::submenu(
            ui,
            Item::new("Columns")
                .icon(Icon::Columns)
                .enabled(activities && self.table_on_screen()),
            |ui| {
                if self.item(
                    ui,
                    "view_columns",
                    Item::new("Customize...").icon(Icon::Columns),
                ) {
                    self.open_columns_dialog();
                }
                let presets = self.column_presets();
                if !presets.is_empty() {
                    separator(ui);
                }
                for (i, (name, columns)) in presets.iter().enumerate() {
                    let label = grid::presets::menu_text(name);
                    let item = Item::new(&label).checked(*columns == self.grid.layout.columns);
                    if self.item(ui, &format!("menu_columns_set_{i}"), item) {
                        self.apply_column_preset(i);
                    }
                }
            },
        );
        if self.item(
            ui,
            "view_timescale",
            Item::new("Ti&mescale...")
                .icon(Icon::Timescale)
                .enabled(activities),
        ) {
            self.open_timescale_dialog();
        }
        separator(ui);
        chrome::submenu(
            ui,
            Item::new("&Filter By")
                .icon(Icon::Filter)
                .enabled(activities),
            |ui| {
                self.filter_items(ui);
            },
        );
        chrome::submenu(
            ui,
            Item::new("&Group and Sort by")
                .icon(Icon::GroupSort)
                .enabled(activities),
            |ui| {
                if self.item(
                    ui,
                    "view_group_sort",
                    Item::new("Customize...").icon(Icon::GroupSort),
                ) {
                    self.open_group_sort_dialog();
                }
                let presets = self.group_presets();
                if !presets.is_empty() {
                    separator(ui);
                }
                for (i, (name, group_sort)) in presets.iter().enumerate() {
                    let label = grid::presets::menu_text(name);
                    let item =
                        Item::new(&label).checked(*group_sort == self.grid.layout.group_sort);
                    if self.item(ui, &format!("menu_group_set_{i}"), item) {
                        self.apply_group_preset(i);
                    }
                }
            },
        );
        separator(ui);
        let spotlight = Item::new("&Progress Spotlight")
            .icon(Icon::Spotlight)
            .enabled(activities)
            .checked(self.gantt.options.spotlight);
        if self.item(ui, "menu_spotlight", spotlight) {
            self.toggle_spotlight();
        }
        let line = Item::new("Progress Line")
            .icon(Icon::ProgressLine)
            .enabled(activities)
            .checked(self.gantt.options.progress_line.show);
        if self.item(ui, "menu_progress_line", line) {
            self.gantt.options.progress_line.show = !self.gantt.options.progress_line.show;
        }
        chrome::submenu(ui, Item::new("A&ttachments").enabled(activities), |ui| {
            let minute = self.chart_center_minute();
            self.attachment_items(ui, "view_attachments", minute);
        });
        let numbers = Item::new("Line Numbers")
            .icon(Icon::LineNumbers)
            .enabled(activities)
            .checked(self.grid.layout.table.line_numbers);
        if self.item(ui, "menu_line_numbers", numbers) {
            self.toggle_line_numbers();
        }
        chrome::submenu(
            ui,
            Item::new("Activity Critical Path").enabled(activities),
            |ui| self.critical_path_items(ui),
        );
        chrome::submenu(ui, Item::new("Chec&k Schedule"), |ui| {
            if self.item(
                ui,
                "menu_view_check_schedule",
                Item::new("Check Schedule...").icon(Icon::CheckSchedule),
            ) {
                self.open_check_schedule();
            }
            for filter in layout_opts::checks::CheckFilter::ALL {
                let item = Item::new(filter.caption())
                    .enabled(activities)
                    .checked(self.check_filter_on(filter));
                let action = format!("menu_{}", filter.key().replace(':', "_"));
                if self.item(ui, &action, item) {
                    self.apply_check_filter(filter);
                }
            }
        });
        // The WBS and Projects windows have their own table and tree.
        let tree_view = matches!(
            self.views.main,
            views::MainView::Wbs | views::MainView::Projects
        );
        if self.item(
            ui,
            "menu_table_font",
            Item::new("Table Fo&nt and Row...").enabled(activities || tree_view),
        ) && !self.wbs_projects_view_command("table_font")
        {
            self.open_table_font();
        }
        separator(ui);
        chrome::submenu(
            ui,
            Item::new("Activity Network")
                .enabled(activities && self.panes.top == panes::TopView::Network),
            |ui| self.network_menu_items(ui),
        );
        if self.item(
            ui,
            "menu_chart_font",
            Item::new("Chart Font and Co&lors...").enabled(activities),
        ) {
            self.open_chart_font();
        }
        chrome::submenu(ui, Item::new("&Run Program"), |ui| self.launch_items(ui));
        separator(ui);
        let hints = Item::new("Cell Tips")
            .key("Alt+F1")
            .icon(Icon::Hint)
            .checked(self.grid.layout.table.hint_help);
        if self.item(ui, "menu_hint_help", hints) {
            self.toggle_hint_help();
        }
        let status = Item::new("&Status Bar").checked(self.views.show_status_bar);
        if self.item(ui, "menu_status_bar", status) {
            self.views.show_status_bar = !self.views.show_status_bar;
        }
        separator(ui);
        let timeline = self.timeline_on_screen();
        chrome::submenu(ui, Item::new("&Zoom").enabled(timeline), |ui| {
            if self.item(ui, "menu_zoom_in", Item::new("Zoom In").icon(Icon::ZoomIn)) {
                self.zoom_step(true);
            }
            if self.item(
                ui,
                "menu_zoom_out",
                Item::new("Zoom Out").icon(Icon::ZoomOut),
            ) {
                self.zoom_step(false);
            }
            if self.item(
                ui,
                "menu_zoom_fit",
                Item::new("Zoom To Best Fit").icon(Icon::Fit),
            ) {
                self.zoom_to_best_fit();
            }
            separator(ui);
            self.zoom_menu(ui);
        });
        separator(ui);
        if self.item(
            ui,
            "menu_expand_all",
            Item::new("&Expand All")
                .icon(Icon::ExpandAll)
                .enabled(activities || tree_view),
        ) && !self.wbs_projects_view_command("expand_all")
        {
            self.expand_all_bands();
        }
        if self.item(
            ui,
            "menu_collapse_all",
            Item::new("&Collapse All")
                .icon(Icon::CollapseAll)
                .enabled(activities || tree_view),
        ) && !self.wbs_projects_view_command("collapse_all")
        {
            self.collapse_all_bands();
        }
        if self.item(
            ui,
            "menu_collapse_to",
            Item::new("Collapse To...")
                .icon(Icon::CollapseTo)
                .enabled(activities || tree_view),
        ) && !self.wbs_projects_view_command("collapse_to")
        {
            self.open_collapse_to();
        }
        chrome::submenu(ui, Item::new("Tool&bars"), |ui| self.toolbars_items(ui));
        if self.item(ui, "menu_reset_toolbars", Item::new("Reset All Toolbars")) {
            self.reset_all_toolbars();
        }
        self.lock_toolbars_item(ui);
        separator(ui);
        chrome::submenu(ui, Item::new("Tab Groups"), |ui| {
            use views::tab_groups::Split;
            let groups = &self.views.groups;
            let (horizontal, vertical, merge) = (
                groups.can_split(Split::Horizontal),
                groups.can_split(Split::Vertical),
                groups.can_merge(),
            );
            if self.item(
                ui,
                "menu_tab_horizontal",
                Item::new("New Horizontal Tab Group")
                    .icon(Icon::TabHorizontal)
                    .enabled(horizontal),
            ) {
                self.new_tab_group(Split::Horizontal);
            }
            if self.item(
                ui,
                "menu_tab_vertical",
                Item::new("New Vertical Tab Group")
                    .icon(Icon::TabVertical)
                    .enabled(vertical),
            ) {
                self.new_tab_group(Split::Vertical);
            }
            if self.item(
                ui,
                "menu_tab_merge",
                Item::new("Merge All Tab Groups")
                    .icon(Icon::TabMerge)
                    .enabled(merge),
            ) {
                self.merge_tab_groups();
            }
        });
    }

    /// Show on Bottom entries, shared by View and the layout options menu.
    pub(super) fn show_on_bottom_items(&mut self, ui: &mut egui::Ui) {
        self.bottom_menu_items(ui, "Details");
    }

    /// Filter By: All Activities, the standard filters, then the Filters
    /// dialog. Picking a filter applies it alone; All Activities clears.
    pub(super) fn filter_items(&mut self, ui: &mut egui::Ui) {
        let all = !self.grid.layout.filter.active();
        if self.item(
            ui,
            "menu_filter_all",
            Item::new("All Activities").checked(all),
        ) {
            self.grid.layout.filter.all_activities = true;
            self.drop_frame_cache();
        }
        separator(ui);
        for standard in grid::filters::Standard::LISTED {
            let key = standard.key();
            let on = !all && self.grid.layout.filter.selected.contains(&key);
            let item = Item::new(standard.caption()).checked(on);
            if self.item(ui, &format!("menu_filter_{key}"), item) {
                let filter = &mut self.grid.layout.filter;
                filter.all_activities = false;
                filter.selected = vec![key];
                self.drop_frame_cache();
            }
        }
        separator(ui);
        if self.item(
            ui,
            "menu_filters",
            Item::new("Filters...").icon(Icon::Filter),
        ) {
            self.open_filters_dialog();
        }
    }

    /// Open, Save and Save As for layouts, shared by View > Layout and the
    /// `Layout:` button's menu.
    pub(super) fn layout_file_items(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            "layout_open",
            Item::new("&Open Layout...").icon(Icon::Open),
        ) {
            self.open_layout_dialog();
        }
        if self.item(
            ui,
            "layout_save",
            Item::new("&Save Layout").icon(Icon::Save),
        ) {
            let name = self.grid.layout.name.clone();
            if let Err(e) = self.save_layout_as(&name) {
                self.error(e);
            }
        }
        if self.item(
            ui,
            "layout_save_as",
            Item::new("Save Layout &As...").icon(Icon::Save),
        ) {
            self.open_save_as_dialog();
        }
    }

    fn toolbars_items(&mut self, ui: &mut egui::Ui) {
        for name in TOOLBAR_LIST {
            let item = Item::new(name).checked(self.toolbar_visible(name));
            if self.item(ui, &format!("menu_toolbar_{name}"), item) {
                self.toggle_toolbar(name);
            }
        }
        separator(ui);
        if self.item(ui, "menu_customize_toolbars", Item::new("&Customize...")) {
            self.open_customize_toolbars();
        }
        if self.item(
            ui,
            "menu_reset_toolbar_layout",
            Item::new("&Reset Toolbars"),
        ) {
            self.reset_toolbar_layout();
        }
    }

    fn project_menu(&mut self, ui: &mut egui::Ui) {
        self.project_menu_views(ui);
    }

    fn admin_menu(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            "menu_admin_preferences",
            Item::new("Admin &Preferences...").icon(Icon::AdminPreferences),
        ) {
            self.open_admin_preferences();
        }
        self.dictionary_item(ui, crate::dictionaries::Dictionary::AdminCategories);
        self.enterprise_item(ui, crate::enterprise::Dialog::Currencies);
        separator(ui);
        self.enterprise_item(ui, crate::enterprise::Dialog::FinCalendars);
        separator(ui);
        // Only a workspace that keeps dictionaries per project can be converted.
        let share = Item::new("&Share Dictionaries Across Projects...")
            .enabled(self.store.shared() == Ok(false));
        if self.item(ui, "menu_share_dictionaries", share) {
            self.open_share_dictionaries();
        }
    }

    fn tools_menu(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            "menu_schedule",
            Item::new("&Schedule...").key("F9").icon(Icon::Schedule),
        ) {
            self.open_schedule_dialog();
        }
        if self.item(
            ui,
            "menu_check_schedule",
            Item::new("&Check Schedule...").icon(Icon::CheckSchedule),
        ) {
            self.open_check_schedule();
        }
        self.progress_tools_menu(ui);
        separator(ui);
        let reorganize = Item::new("Pause Auto-Sor&t")
            .key("Shift+F12")
            .icon(Icon::Reorganize)
            .checked(self.layout_opts.reorganize_off);
        if self.item(ui, "menu_reorganize", reorganize) {
            self.toggle_reorganize();
        }
        self.menu_item_icon(
            ui,
            "tools_global_change",
            "&Global Change...",
            "",
            true,
            editing::Command::GlobalChange,
            Some(Icon::GlobalChange),
        );
        separator(ui);
        if self.item(
            ui,
            "menu_monitor_thresholds",
            Item::new("&Monitor Thresholds...").icon(Icon::Thresholds),
        ) {
            self.open_monitor_thresholds();
        }
        if self.item(
            ui,
            "menu_issue_navigator",
            Item::new("&Issue Navigator...").icon(Icon::Issues),
        ) {
            self.open_issue_navigator();
        }
        separator(ui);
        self.reports_menu_items(ui);
        self.project_checker_menu_item(ui);
        separator(ui);
        if self.item(
            ui,
            "menu_top_down",
            Item::new("Spread &Budget Hours (Top-Down)..."),
        ) {
            self.open_top_down_estimation();
        }
        self.compare_menu_item(ui);
    }

    fn help_menu(&mut self, ui: &mut egui::Ui) {
        // Help Topics was "Local Help": OpenXER's own topics, offline.
        if self.item(
            ui,
            "menu_local_help",
            Item::new("&Help Topics...").key("Ctrl+F1").icon(Icon::Help),
        ) {
            self.open_help_contents();
        }
        // About in its own group after the first item, as in the reference.
        separator(ui);
        if self.item(ui, "menu_about", Item::new("&About OpenXER...")) {
            self.show_about = true;
        }
        separator(ui);
        if self.item(
            ui,
            "menu_context_help",
            Item::new("Help on This &View...")
                .key("F1")
                .icon(Icon::Help),
        ) {
            let topic = help::view_topic(self.views.groups.current());
            self.help.go(topic);
        }
        // The release notes (CHANGELOG.md), newest release first.
        if self.item(
            ui,
            "menu_whats_new",
            Item::new("&What's New...").icon(Icon::WhatsNew),
        ) {
            self.open_whats_new();
        }
    }

    /// Zoom In / Zoom Out around the chart center along the measured zoom
    /// ladder shared with the Gantt (`timescale::ZOOM_LADDER`).
    pub(super) fn zoom_step(&mut self, zoom_in: bool) {
        // The focused timeline: the top or bottom Gantt (or the Activity
        // Network), or the current view's own chart.
        self.focused_zoom(zoom_in);
    }

    // ------------------------------------------------------------ toolbars

    /// The buttons of toolbar `name`, laid out by whatever layout `ui` has.
    pub(crate) fn bar_buttons(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, name: &str) {
        match name {
            "Print" => self.print_bar(ui),
            "Top Layout" => self.top_layout_bar(ui),
            "Bottom Layout" => self.bottom_layout_bar(ui),
            "Layout" => self.layout_bar_buttons(ui),
            "Tools" => self.tools_bar(ui),
            "Display" => self.display_bar(ui, ctx),
            "Dictionaries" => self.dictionaries_bar(ui),
            "Activity Critical Path" => self.critical_path_bar(ui),
            "Action" => self.action_bar(ui),
            "Administrator" => self.administrator_bar(ui),
            "Find" => self.find_bar(ui),
            "Reports" => self.reports_bar(ui),
            "Check Schedule" => self.check_schedule_bar(ui),
            "Standard" | "Enterprise" | "Project" | "Edit" | "Assign" | "Move" => {
                self.directory_bar(ui, name)
            }
            _ => {}
        }
    }

    /// The directory and edit bars: buttons from the lists the docks build
    /// each frame; a click is acted on once the docks are drawn.
    fn directory_bar(&mut self, ui: &mut egui::Ui, name: &str) {
        let Some(buttons) = self.views.bars.rt.specs.get(name).cloned() else {
            return;
        };
        for (action, icon, tip, enabled) in buttons {
            if self.tool(ui, action, icon, tip, enabled) {
                self.views.bars.rt.clicked = Some(action);
            }
        }
    }

    fn print_bar(&mut self, ui: &mut egui::Ui) {
        let printable = self.can_print();
        let can_print = printable || self.current_report_tab().is_some();
        if self.tool(ui, "tool_print", Icon::Print, "Print", can_print) {
            self.print_command();
        }
        if self.tool_ex(
            ui,
            "tool_print_preview",
            Icon::PrintPreview,
            "Print Preview",
            printable,
            false,
            false,
        ) {
            self.show_print_preview();
        }
    }

    fn top_layout_bar(&mut self, ui: &mut egui::Ui) {
        let activities = self.views.main.is_activities();
        let table = self.panes.top == panes::TopView::Table;
        if self.tool_ex(
            ui,
            "tool_table",
            Icon::Table,
            "Table",
            activities,
            activities && table,
            false,
        ) {
            self.set_top(panes::TopView::Table);
        }
        let network = self.panes.top == panes::TopView::Network;
        if self.tool_ex(
            ui,
            "tool_gantt",
            Icon::Gantt,
            "Gantt Chart",
            activities,
            activities && self.panes.top == panes::TopView::Gantt,
            false,
        ) {
            self.set_top(panes::TopView::Gantt);
        }
        if self.tool_ex(
            ui,
            "tool_network",
            Icon::Network,
            "Activity Network",
            activities,
            activities && network,
            false,
        ) {
            self.set_top(panes::TopView::Network);
        }
        if self.tool_ex(
            ui,
            "links",
            Icon::Links,
            "Relationship Lines",
            activities,
            activities && self.gantt.options.show_relationships,
            false,
        ) {
            self.gantt.options.show_relationships = !self.gantt.options.show_relationships;
        }
    }

    fn bottom_layout_bar(&mut self, ui: &mut egui::Ui) {
        self.bottom_tool_buttons(ui);
    }

    fn layout_bar_buttons(&mut self, ui: &mut egui::Ui) {
        let activities = self.views.main.is_activities();
        let bars = activities && self.gantt_on_screen();
        let columns = activities && self.table_on_screen();
        if self.tool(ui, "tool_bars", Icon::Bars, "Bars", bars) {
            self.open_bars_dialog();
        }
        if self.tool_ex(
            ui,
            "tool_columns",
            Icon::Columns,
            "Columns",
            columns,
            false,
            true,
        ) {
            self.open_columns_dialog();
        }
        if self.tool(ui, "timescale", Icon::Timescale, "Timescale", activities) {
            self.open_timescale_dialog();
        }
        let filter = self
            .button_visible("tool_filter")
            .then(|| chrome::tool_button(ui, Icon::Filter, "Filter By", activities, false, true));
        if let Some(filter) = filter {
            #[cfg(test)]
            self.views
                .bars
                .rt
                .drawn_icons
                .insert("tool_filter".into(), Icon::Filter);
            self.note_button_width("tool_filter", filter.rect.width());
            self.action("tool_filter", &filter);
            if activities {
                egui::Popup::menu(&filter)
                    .style(chrome::bar_menu_style)
                    .show(|ui| self.filter_items(ui));
            }
        }
        if self.tool_ex(
            ui,
            "tool_group_sort",
            Icon::GroupSort,
            "Group and Sort by",
            activities,
            false,
            true,
        ) {
            self.open_group_sort_dialog();
        }
        if self.tool_ex(
            ui,
            "tool_line_numbers",
            Icon::LineNumbers,
            "Line Numbers",
            activities,
            activities && self.grid.layout.table.line_numbers,
            false,
        ) {
            self.toggle_line_numbers();
        }
    }

    fn tools_bar(&mut self, ui: &mut egui::Ui) {
        if self.tool(ui, "schedule", Icon::Schedule, "Schedule (F9)", true) {
            self.open_schedule_dialog();
        }
        if self.tool(
            ui,
            "tool_level",
            Icon::Level,
            "Level Resources (Shift+F9)",
            true,
        ) {
            self.open_level_resources();
        }
        // As View > Progress Spotlight and Progress Line: the Activities view.
        let activities = self.views.main.is_activities();
        let spotlight = self.gantt.options.spotlight;
        if self.tool_ex(
            ui,
            "tool_spotlight",
            Icon::Spotlight,
            "Progress Spotlight",
            activities,
            spotlight,
            false,
        ) {
            self.toggle_spotlight();
        }
        let line = self.gantt.options.progress_line.show;
        if self.tool_ex(
            ui,
            "tool_progress_line",
            Icon::ProgressLine,
            "Progress Line",
            activities,
            line,
            false,
        ) {
            self.gantt.options.progress_line.show = !line;
        }
        if self.tool(
            ui,
            "tool_update_progress",
            Icon::UpdateProgress,
            "Update Progress",
            true,
        ) {
            self.open_update_progress();
        }
        let off = self.layout_opts.reorganize_off;
        if self.tool_ex(
            ui,
            "tool_auto_reorganize",
            Icon::Reorganize,
            "Pause Auto-Sort (Shift+F12)",
            true,
            off,
            false,
        ) {
            self.toggle_reorganize();
        }
    }

    fn display_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let timeline = self.timeline_on_screen();
        if self.tool(ui, "zoom_in", Icon::ZoomIn, "Zoom In", timeline) {
            self.zoom_step(true);
        }
        if self.tool(ui, "zoom_out", Icon::ZoomOut, "Zoom Out", timeline) {
            self.zoom_step(false);
        }
        if self.tool(ui, "fit", Icon::Fit, "Zoom To Best Fit", timeline) {
            self.zoom_to_best_fit();
        }
        use views::tab_groups::Split;
        let groups = &self.views.groups;
        let (horizontal, vertical, merge) = (
            groups.can_split(Split::Horizontal),
            groups.can_split(Split::Vertical),
            groups.can_merge(),
        );
        if self.tool(
            ui,
            "tool_tab_horizontal",
            Icon::TabHorizontal,
            "New Horizontal Tab Group",
            horizontal,
        ) {
            self.new_tab_group(Split::Horizontal);
        }
        if self.tool(
            ui,
            "tool_tab_merge",
            Icon::TabMerge,
            "Merge All Tab Groups",
            merge,
        ) {
            self.merge_tab_groups();
        }
        if self.tool(
            ui,
            "tool_tab_vertical",
            Icon::TabVertical,
            "New Vertical Tab Group",
            vertical,
        ) {
            self.new_tab_group(Split::Vertical);
        }
        let hints = self.grid.layout.table.hint_help;
        if self.tool_ex(
            ui,
            "tool_hints",
            Icon::Hint,
            "Cell Tips (Alt+F1)",
            true,
            hints,
            false,
        ) {
            self.toggle_hint_help();
        }
        if self.tool(
            ui,
            "tool_local_help",
            Icon::Help,
            "Help Topics (Ctrl+F1)",
            true,
        ) {
            self.open_help_contents();
        }
        if self.tool(ui, "tool_whats_new", Icon::WhatsNew, "What's New", true) {
            self.open_whats_new();
        }
        let _ = ctx;
    }

    fn dictionaries_bar(&mut self, ui: &mut egui::Ui) {
        for (action, icon, tip) in [
            ("tool_resource_codes", Icon::ResourceCodes, "Resource Codes"),
            ("tool_project_codes", Icon::ProjectCodes, "Project Codes"),
            ("tool_activity_codes", Icon::ActivityCodes, "Activity Codes"),
            ("tool_role_codes", Icon::RoleCodes, "Role Codes"),
            (
                "tool_assignment_codes",
                Icon::AssignmentCodes,
                "Assignment Codes",
            ),
            ("tool_udf", Icon::Udf, "User Defined Fields"),
        ] {
            self.dictionary_tool(ui, action, icon, tip);
        }
        if self.tool(ui, "tool_calendars", Icon::Calendars, "Calendars", true) {
            self.open_calendars();
        }
        for (action, icon, tip) in [
            (
                "tool_step_templates",
                Icon::StepTemplates,
                "Activity Step Templates",
            ),
            ("tool_cost_accounts", Icon::CostAccounts, "Cost Accounts"),
            (
                "tool_funding_sources",
                Icon::FundingSources,
                "Funding Sources",
            ),
        ] {
            self.dictionary_tool(ui, action, icon, tip);
        }
    }
}

// The optional toolbars, off until switched on: each one carries the
// commands of the menu group it is named after.
impl App {
    /// Action: the progress and data commands of the Tools menu.
    fn action_bar(&mut self, ui: &mut egui::Ui) {
        if self.tool(
            ui,
            "tool_apply_actuals",
            Icon::ApplyActuals,
            "Apply Actuals",
            true,
        ) {
            self.open_apply_actuals();
        }
        if self.tool(
            ui,
            "tool_store_period",
            Icon::StorePeriod,
            "Save Period Actuals",
            true,
        ) {
            self.sched.progress.store_period = true;
        }
        if self.tool(
            ui,
            "tool_recalc_costs",
            Icon::RecalcCosts,
            "Recalculate Assignment Costs",
            true,
        ) {
            self.open_recalc_costs();
        }
        if self.tool(
            ui,
            "tool_global_change",
            Icon::GlobalChange,
            "Global Change",
            true,
        ) {
            self.editing.pending = Some(editing::Command::GlobalChange);
        }
    }

    /// Administrator: the Admin menu.
    fn administrator_bar(&mut self, ui: &mut egui::Ui) {
        if self.tool(
            ui,
            "tool_admin_preferences",
            Icon::AdminPreferences,
            "Admin Preferences",
            true,
        ) {
            self.open_admin_preferences();
        }
        if self.tool(
            ui,
            "tool_admin_categories",
            Icon::AdminCategories,
            "Categories and Lists",
            true,
        ) {
            self.open_dictionary(crate::dictionaries::Dictionary::AdminCategories);
        }
        if self.tool(ui, "tool_currencies", Icon::Currencies, "Currencies", true) {
            self.open_enterprise(crate::enterprise::Dialog::Currencies);
        }
        if self.tool(
            ui,
            "tool_fin_calendars",
            Icon::Calendars,
            "Financial Period Calendars",
            true,
        ) {
            self.open_enterprise(crate::enterprise::Dialog::FinCalendars);
        }
    }

    /// Find: Find, Find Next and Replace of the Edit menu.
    fn find_bar(&mut self, ui: &mut egui::Ui) {
        let activities = self.views.main.is_activities();
        if self.tool(ui, "tool_find", Icon::Find, "Find (Ctrl+F)", true) {
            self.open_find(false);
        }
        let findable = activities || self.view_find_ready();
        let next = findable && self.find_ready();
        if self.tool(ui, "tool_find_next", Icon::FindNext, "Find Next (F3)", next) {
            self.run_find_next();
        }
        if self.tool(
            ui,
            "tool_replace",
            Icon::Replace,
            "Replace (Ctrl+R)",
            findable,
        ) {
            self.open_find(true);
        }
    }

    /// Reports: the eight report commands. Run Report, Run Batch, Modify and
    /// Export act on the report selected in the Reports view.
    fn reports_bar(&mut self, ui: &mut egui::Ui) {
        let entry = self.report_selected_entry();
        let runnable = entry.as_ref().is_some_and(|e| self.can_run_entry(e));
        let stored = entry
            .as_ref()
            .is_some_and(|e| matches!(e.which, reports::rpt_entries::Which::Stored(_)));
        for (action, icon, tip, enabled, command) in [
            (
                "tool_run_report",
                Icon::Run,
                "Run Report",
                runnable,
                "run_report",
            ),
            (
                "tool_run_batch",
                Icon::RunBatch,
                "Run Batch",
                true,
                "run_batch",
            ),
            (
                "tool_modify_report",
                Icon::Modify,
                "Modify Report",
                entry.as_ref().is_some_and(|e| !e.is_builtin()),
                "modify",
            ),
            (
                "tool_import_report",
                Icon::Import,
                "Import Report",
                true,
                "import",
            ),
            (
                "tool_export_report",
                Icon::Export,
                "Export Report",
                stored,
                "export",
            ),
            (
                "tool_report_wizard",
                Icon::ReportWizard,
                "Report Wizard",
                true,
                "wizard",
            ),
            (
                "tool_report_groups",
                Icon::ReportGroups,
                "Report Groups",
                true,
                "groups",
            ),
            (
                "tool_batch_reports",
                Icon::BatchReports,
                "Batch Reports",
                true,
                "batches",
            ),
        ] {
            if self.tool(ui, action, icon, tip, enabled) {
                self.reports_command(command);
            }
        }
    }

    /// Check Schedule: the options and report, and a dropdown of the check
    /// filters of View > Check Schedule.
    fn check_schedule_bar(&mut self, ui: &mut egui::Ui) {
        if self.tool(
            ui,
            "tool_check_schedule",
            Icon::CheckSchedule,
            "Check Schedule",
            true,
        ) {
            self.open_check_schedule();
        }
        let activities = self.views.main.is_activities();
        let filters = self.button_visible("tool_check_filters").then(|| {
            chrome::tool_button(
                ui,
                Icon::CheckFilters,
                "Check Schedule Filters",
                activities,
                false,
                true,
            )
        });
        if let Some(filters) = filters {
            #[cfg(test)]
            self.views
                .bars
                .rt
                .drawn_icons
                .insert("tool_check_filters".into(), Icon::CheckFilters);
            self.action("tool_check_filters", &filters);
            if activities {
                egui::Popup::menu(&filters)
                    .style(chrome::bar_menu_style)
                    .show(|ui| {
                        for filter in layout_opts::checks::CheckFilter::ALL {
                            let item =
                                Item::new(filter.caption()).checked(self.check_filter_on(filter));
                            let action = format!("tool_{}", filter.key().replace(':', "_"));
                            if self.item(ui, &action, item) {
                                self.apply_check_filter(filter);
                            }
                        }
                    });
            }
        }
    }
}

/// Test support: the menu that holds a command that has no toolbar button,
/// so headless tests can open it before clicking the command.
#[cfg(test)]
pub fn menu_holding(action: &str) -> Option<&'static str> {
    match action {
        "undo" | "redo" | "menu_user_preferences" | "menu_find" => Some("Edit"),
        "save" | "file_import" | "file_export" | "menu_refresh" | "menu_new" => Some("File"),
        "menu_schedule"
        | "menu_check_schedule"
        | "menu_monitor_thresholds"
        | "menu_issue_navigator"
        | "menu_top_down" => Some("Tools"),
        "menu_expenses" | "menu_documents" | "menu_risks" | "menu_issues" | "menu_thresholds" => {
            Some("Project")
        }
        "menu_external_apps" => Some("Enterprise"),
        _ => crate::enterprise::Dialog::ALL
            .iter()
            .find(|d| d.action() == action)
            .map(|d| d.menu()),
    }
}

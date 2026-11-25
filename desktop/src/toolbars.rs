//! View > Toolbars (switching bars on and off), Customize..., Reset All
//! Toolbars and Lock All Toolbars. The Customize
//! dialog shows or hides each button of each toolbar; Reset puts the selected toolbar's buttons
//! back. Changes apply at once and are kept in the workspace preferences
//! with the toolbar choices; Close or Escape closes the dialog. Lock All
//! Toolbars hides the toolbars' drag grips and turns off moving them (see
//! `dockbars.rs`); Reset Toolbars puts every bar back at its default place.
//! The chevron at the end of each bar opens a menu over the same hidden
//! button state: Add or Remove Buttons, Customize... on that bar, and Reset
//! Toolbar. Nothing here touches the project.
use super::*;
use chrome::{pal, Icon, Item};

/// Every toolbar's buttons: (bar, [(action, caption)]), in bar order.
pub const TOOLBAR_BUTTONS: [(&str, &[(&str, &str)]); 19] = [
    (
        "Print",
        &[
            ("tool_print", "Print"),
            ("tool_print_preview", "Print Preview"),
        ],
    ),
    (
        "Top Layout",
        &[
            ("tool_table", "Table"),
            ("tool_gantt", "Gantt Chart"),
            ("tool_network", "Activity Network"),
            ("links", "Relationship Lines"),
        ],
    ),
    (
        "Bottom Layout",
        &[
            ("details", "Details"),
            ("tool_activity_usage_sheet", "Activity Usage Spreadsheet"),
            ("tool_activity_usage_profile", "Activity Usage Profile"),
            ("tool_resource_usage_sheet", "Resource Usage Spreadsheet"),
            ("tool_resource_usage_profile", "Resource Usage Profile"),
            ("trace", "Trace Logic"),
        ],
    ),
    (
        "Layout",
        &[
            ("tool_bars", "Bars"),
            ("tool_columns", "Columns"),
            ("timescale", "Timescale"),
            ("tool_filter", "Filter By"),
            ("tool_group_sort", "Group and Sort by"),
            ("tool_line_numbers", "Line Numbers"),
        ],
    ),
    (
        "Tools",
        &[
            ("schedule", "Schedule"),
            ("tool_level", "Level Resources"),
            ("tool_spotlight", "Progress Spotlight"),
            ("tool_progress_line", "Progress Line"),
            ("tool_update_progress", "Update Progress"),
            ("tool_auto_reorganize", "Pause Auto-Sort"),
        ],
    ),
    (
        "Display",
        &[
            ("zoom_in", "Zoom In"),
            ("zoom_out", "Zoom Out"),
            ("fit", "Zoom To Best Fit"),
            ("tool_tab_horizontal", "New Horizontal Tab Group"),
            ("tool_tab_merge", "Merge All Tab Groups"),
            ("tool_tab_vertical", "New Vertical Tab Group"),
            ("tool_hints", "Cell Tips"),
            ("tool_local_help", "Help Topics"),
            ("tool_whats_new", "What's New"),
        ],
    ),
    (
        "Dictionaries",
        &[
            ("tool_resource_codes", "Resource Codes"),
            ("tool_project_codes", "Project Codes"),
            ("tool_activity_codes", "Activity Codes"),
            ("tool_role_codes", "Role Codes"),
            ("tool_assignment_codes", "Assignment Codes"),
            ("tool_udf", "User Defined Fields"),
            ("tool_calendars", "Calendars"),
            ("tool_step_templates", "Activity Step Templates"),
            ("tool_cost_accounts", "Cost Accounts"),
            ("tool_funding_sources", "Funding Sources"),
        ],
    ),
    (
        "Standard",
        &[("new", "New"), ("open", "Open"), ("close_all", "Close All")],
    ),
    (
        "Enterprise",
        &[
            ("directory_projects", "Projects"),
            ("directory_resources", "Resources"),
            ("dock_reports", "Reports"),
        ],
    ),
    (
        "Project",
        &[
            ("directory_activities", "Activities"),
            ("dock_additional_activities", "Additional Activities View"),
            ("directory_wbs", "WBS"),
            ("dock_resource_assignments", "Resource Assignments"),
            ("dock_wps_docs", "Documents"),
            ("dock_expenses", "Expenses"),
            ("dock_risks", "Risks"),
        ],
    ),
    (
        "Edit",
        &[
            ("add", "Add"),
            ("dock_delete", "Delete"),
            ("dock_cut", "Cut"),
            ("dock_copy", "Copy"),
            ("dock_paste", "Paste"),
        ],
    ),
    (
        "Assign",
        &[
            ("dock_assign_resources", "Resources"),
            ("dock_assign_by_role", "Resources by Role"),
            ("dock_assign_roles", "Roles"),
            ("dock_assign_codes", "Activity Codes"),
            ("dock_assign_predecessors", "Predecessors"),
            ("dock_assign_successors", "Successors"),
            ("dock_assign_steps", "Steps"),
        ],
    ),
    (
        "Move",
        &[
            ("dock_move_up", "Move Up"),
            ("dock_move_down", "Move Down"),
            ("dock_move_left", "Move Left"),
            ("dock_move_right", "Move Right"),
        ],
    ),
    (
        "Activity Critical Path",
        &[
            ("tool_acp_more", "Show More Paths"),
            ("tool_acp_fewer", "Show Fewer Paths"),
            ("tool_acp_select", "Select Paths to Show"),
            ("tool_acp_group", "Enable Group and Sort"),
            ("tool_acp_backward", "Show Backward Activity Critical Paths"),
            ("tool_acp_forward", "Show Forward Activity Critical Paths"),
            ("tool_acp_show", "Show Activity Critical Paths"),
        ],
    ),
    (
        "Action",
        &[
            ("tool_apply_actuals", "Apply Actuals"),
            ("tool_store_period", "Save Period Actuals"),
            ("tool_recalc_costs", "Recalculate Assignment Costs"),
            ("tool_global_change", "Global Change"),
        ],
    ),
    (
        "Administrator",
        &[
            ("tool_admin_preferences", "Admin Preferences"),
            ("tool_admin_categories", "Categories and Lists"),
            ("tool_currencies", "Currencies"),
            ("tool_fin_calendars", "Financial Period Calendars"),
        ],
    ),
    (
        "Find",
        &[
            ("tool_find", "Find"),
            ("tool_find_next", "Find Next"),
            ("tool_replace", "Replace"),
        ],
    ),
    (
        "Reports",
        &[
            ("tool_run_report", "Run Report"),
            ("tool_run_batch", "Run Batch"),
            ("tool_modify_report", "Modify Report"),
            ("tool_import_report", "Import Report"),
            ("tool_export_report", "Export Report"),
            ("tool_report_wizard", "Report Wizard"),
            ("tool_report_groups", "Report Groups"),
            ("tool_batch_reports", "Batch Reports"),
        ],
    ),
    (
        "Check Schedule",
        &[
            ("tool_check_schedule", "Check Schedule"),
            ("tool_check_filters", "Check Schedule Filters"),
        ],
    ),
];

/// Toolbars a fresh setup starts switched off (as if unticked in View >
/// Toolbars). With the reference defaults Dictionaries is off, so the top
/// dock shows one toolbar row (Print, Top Layout, Bottom Layout, Layout,
/// Tools, Display) as the reference's first launch does; the earlier
/// (legacy) defaults showed it on a second row. Switched on, it takes its
/// place on the second row again. A stored choice is read as stored.
pub fn default_hidden_toolbars() -> BTreeSet<String> {
    if crate::defaults::reference() {
        BTreeSet::from(["Dictionaries".to_string()])
    } else {
        BTreeSet::new()
    }
}

/// Is `action` a toolbar button the Customize dialog lists?
pub fn is_button(action: &str) -> bool {
    TOOLBAR_BUTTONS
        .iter()
        .any(|(_, buttons)| buttons.iter().any(|(a, _)| *a == action))
}

/// Where toolbar `name` is in [`TOOLBAR_BUTTONS`]; `None` for the menu bar.
pub fn bar_index(name: &str) -> Option<usize> {
    TOOLBAR_BUTTONS.iter().position(|(bar, _)| *bar == name)
}

/// The icon toolbar button `action` draws on its bar, for the lists that
/// show a button that may be hidden (the chevron's Add or Remove Buttons).
/// A test checks it against what the bars paint.
pub fn button_icon(action: &str) -> Option<Icon> {
    Some(match action {
        "tool_print" => Icon::Print,
        "tool_print_preview" => Icon::PrintPreview,
        "tool_table" => Icon::Table,
        "tool_gantt" => Icon::Gantt,
        "tool_network" => Icon::Network,
        "links" => Icon::Links,
        "details" => Icon::Details,
        "tool_activity_usage_sheet" => Icon::ActivityUsageSheet,
        "tool_activity_usage_profile" => Icon::ActivityUsageProfile,
        "tool_resource_usage_sheet" => Icon::ResourceUsageSheet,
        "tool_resource_usage_profile" => Icon::ResourceUsageProfile,
        "trace" => Icon::Trace,
        "tool_bars" => Icon::Bars,
        "tool_columns" => Icon::Columns,
        "timescale" => Icon::Timescale,
        "tool_filter" => Icon::Filter,
        "tool_group_sort" => Icon::GroupSort,
        "tool_line_numbers" => Icon::LineNumbers,
        "schedule" => Icon::Schedule,
        "tool_level" => Icon::Level,
        "tool_spotlight" => Icon::Spotlight,
        "tool_progress_line" => Icon::ProgressLine,
        "tool_update_progress" => Icon::UpdateProgress,
        "tool_auto_reorganize" => Icon::Reorganize,
        "zoom_in" => Icon::ZoomIn,
        "zoom_out" => Icon::ZoomOut,
        "fit" => Icon::Fit,
        "tool_tab_horizontal" => Icon::TabHorizontal,
        "tool_tab_merge" => Icon::TabMerge,
        "tool_tab_vertical" => Icon::TabVertical,
        "tool_hints" => Icon::Hint,
        "tool_local_help" => Icon::Help,
        "tool_whats_new" => Icon::WhatsNew,
        "tool_resource_codes" => Icon::ResourceCodes,
        "tool_project_codes" => Icon::ProjectCodes,
        "tool_activity_codes" => Icon::ActivityCodes,
        "tool_role_codes" => Icon::RoleCodes,
        "tool_assignment_codes" => Icon::AssignmentCodes,
        "tool_udf" => Icon::Udf,
        "tool_calendars" => Icon::Calendars,
        "tool_step_templates" => Icon::StepTemplates,
        "tool_cost_accounts" => Icon::CostAccounts,
        "tool_funding_sources" => Icon::FundingSources,
        "new" => Icon::New,
        "open" => Icon::Open,
        "close_all" => Icon::CloseAll,
        "directory_projects" => Icon::Projects,
        "directory_resources" => Icon::Resources,
        "dock_reports" => Icon::Reports,
        "directory_activities" | "dock_additional_activities" => Icon::Activities,
        "directory_wbs" => Icon::Wbs,
        "dock_resource_assignments" | "dock_assign_resources" => Icon::AssignResource,
        "dock_wps_docs" => Icon::WorkProducts,
        "dock_expenses" => Icon::Expenses,
        "dock_risks" => Icon::Risks,
        "add" => Icon::Add,
        "dock_delete" => Icon::Delete,
        "dock_cut" => Icon::Cut,
        "dock_copy" => Icon::Copy,
        "dock_paste" => Icon::Paste,
        "dock_assign_by_role" => Icon::AssignByRole,
        "dock_assign_roles" => Icon::AssignRole,
        "dock_assign_codes" => Icon::AssignCode,
        "dock_assign_predecessors" => Icon::Predecessors,
        "dock_assign_successors" => Icon::Successors,
        "dock_assign_steps" => Icon::Steps,
        "dock_move_up" => Icon::ShiftUp,
        "dock_move_down" => Icon::ShiftDown,
        "dock_move_left" => Icon::ShiftLeft,
        "dock_move_right" => Icon::ShiftRight,
        "tool_acp_more" => Icon::AcpMore,
        "tool_acp_fewer" => Icon::AcpFewer,
        "tool_acp_select" => Icon::AcpSelect,
        "tool_acp_group" => Icon::AcpGroup,
        "tool_acp_backward" => Icon::AcpBack,
        "tool_acp_forward" => Icon::AcpForward,
        "tool_acp_show" => Icon::AcpShow,
        "tool_apply_actuals" => Icon::ApplyActuals,
        "tool_store_period" => Icon::StorePeriod,
        "tool_recalc_costs" => Icon::RecalcCosts,
        "tool_global_change" => Icon::GlobalChange,
        "tool_admin_preferences" => Icon::AdminPreferences,
        "tool_admin_categories" => Icon::AdminCategories,
        "tool_currencies" => Icon::Currencies,
        "tool_fin_calendars" => Icon::Calendars,
        "tool_find" => Icon::Find,
        "tool_find_next" => Icon::FindNext,
        "tool_replace" => Icon::Replace,
        "tool_run_report" => Icon::Run,
        "tool_run_batch" => Icon::RunBatch,
        "tool_modify_report" => Icon::Modify,
        "tool_import_report" => Icon::Import,
        "tool_export_report" => Icon::Export,
        "tool_report_wizard" => Icon::ReportWizard,
        "tool_report_groups" => Icon::ReportGroups,
        "tool_batch_reports" => Icon::BatchReports,
        "tool_check_schedule" => Icon::CheckSchedule,
        "tool_check_filters" => Icon::CheckFilters,
        _ => return None,
    })
}

/// The Customize dialog while open: the toolbar whose buttons it lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Customize {
    pub bar: usize,
}

/// A bar's chevron: where it is and which way its menu opens.
pub(crate) struct Chevron<'a> {
    /// The bar it belongs to.
    pub bar: &'a str,
    /// Names the control: its action is `<key>_<bar>`.
    pub key: &'static str,
    /// The area that takes the click and shows the hot frame.
    pub hit: Rect,
    /// The top-left pixel of the triangle.
    pub glyph: Pos2,
    /// The triangle points down (a horizontal bar) or right.
    pub down: bool,
    /// Which way the menu opens from the chevron.
    pub align: egui::RectAlign,
}

impl App {
    /// The chevron at the end of a bar: hot like a toolbar button, and a
    /// click opens (or, while it is open, closes) the bar's options menu.
    /// Locking the toolbars only stops them moving, so the menu works while
    /// they are locked too.
    pub(crate) fn bar_chevron(&mut self, ui: &mut egui::Ui, chevron: Chevron) {
        let Chevron {
            bar,
            key,
            hit,
            glyph,
            down,
            align,
        } = chevron;
        let response = ui.interact(hit, egui::Id::new((key, bar)), Sense::click());
        self.action(&format!("{key}_{bar}"), &response);
        let ctx = ui.ctx().clone();
        let open = egui::Popup::is_id_open(&ctx, egui::Popup::default_response_id(&response));
        let painter = ui.painter();
        if response.is_pointer_button_down_on() {
            chrome::hot_frame(painter, hit, pal::PRESSED_FILL, pal::PRESSED_BORDER);
        } else if open || response.hovered() {
            chrome::hot_frame(painter, hit, pal::HOT_FILL, pal::HOT_BORDER);
        }
        chrome::chevron(painter, glyph, down, INK);
        egui::Popup::menu(&response)
            .style(chrome::bar_menu_style)
            .align(align)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let mut m = context_menu::Menu::on(ui, "chevron");
                self.chevron_menu(&mut m, bar);
            });
    }

    /// The chevron's menu: Add or Remove Buttons (the bar's buttons, each
    /// checked while shown; a click switches it and the menu stays open),
    /// Customize... (the dialog on this bar) and Reset Toolbar (this bar's
    /// buttons all shown again). The menu bar has no buttons to list.
    fn chevron_menu(&mut self, m: &mut context_menu::Menu, bar: &str) {
        let index = bar_index(bar);
        let add_remove = Item::new("&Add or Remove Buttons").enabled(index.is_some());
        m.submenu_kept_open("add_remove", add_remove, |m| {
            let Some(index) = index else { return };
            for (action, caption) in TOOLBAR_BUTTONS[index].1 {
                let shown = self.button_visible(action);
                let label = caption.replace('&', "&&");
                let key = format!("button_{action}");
                if m.toggle_icon(&key, &label, button_icon(action), shown) {
                    self.set_button_shown(action, !shown);
                }
            }
        });
        m.separator();
        if m.item("customize", Item::new("&Customize...")) {
            self.open_customize_toolbars_for(bar);
        }
        if m.item_if("reset", Item::new("&Reset Toolbar"), index.is_some()) {
            if let Some(index) = index {
                self.reset_bar_buttons(index);
            }
        }
    }

    /// Shows or hides toolbar button `action`: the state the Customize
    /// dialog and the chevron menus share.
    pub(crate) fn set_button_shown(&mut self, action: &str, shown: bool) {
        if shown {
            self.views.hidden_buttons.remove(action);
        } else {
            self.views.hidden_buttons.insert(action.to_string());
        }
    }

    /// Every button of toolbar `index` shown again; the other bars keep
    /// theirs.
    pub(crate) fn reset_bar_buttons(&mut self, index: usize) {
        for (action, _) in TOOLBAR_BUTTONS[index].1 {
            self.views.hidden_buttons.remove(*action);
        }
    }

    /// The Customize dialog with toolbar `bar` selected (the first toolbar
    /// for the menu bar, which it does not list).
    pub(crate) fn open_customize_toolbars_for(&mut self, bar: &str) {
        self.views.customize = Some(Customize {
            bar: bar_index(bar).unwrap_or(0),
        });
    }

    /// The popup a right-click on a toolbar area opens: the toolbars with a
    /// check mark each (the same state as View > Toolbars), then
    /// Customize..., as in the reference (Status Bar is in the View menu).
    /// `rect` is the dock's area, `which` names the dock. The
    /// popup hangs on a zero-size anchor so it opens wherever the pointer
    /// was, over a button or beside it.
    pub(crate) fn toolbar_context_menu(&mut self, ui: &mut egui::Ui, rect: Rect, which: &str) {
        let at = ui
            .input(|i| {
                i.pointer
                    .button_clicked(egui::PointerButton::Secondary)
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            })
            .filter(|pos| rect.contains(*pos) && ui.rect_contains_pointer(rect));
        let anchor = ui.interact(
            Rect::from_min_size(at.unwrap_or(Pos2::ZERO), Vec2::ZERO),
            egui::Id::new(("toolbar_context_menu", which)),
            Sense::hover(),
        );
        context_menu::ContextMenu::new("toolbar_popup")
            .min_width(150.)
            .open_at(at)
            .show(&anchor, self, |app, m| {
                for name in menu::TOOLBAR_LIST {
                    let on = app.toolbar_visible(name);
                    if m.check(&format!("toolbar_{name}"), name, on) {
                        app.toggle_toolbar(name);
                    }
                }
                m.separator();
                if m.item("customize", Item::new("Customize...")) {
                    app.open_customize_toolbars();
                }
            });
    }

    /// Is toolbar button `action` shown (not hidden through Customize)?
    pub(crate) fn button_visible(&self, action: &str) -> bool {
        !self.views.hidden_buttons.contains(action)
    }

    /// View > Toolbars > Customize...
    pub(crate) fn open_customize_toolbars(&mut self) {
        self.views.customize = Some(Customize { bar: 0 });
    }

    /// View > Reset All Toolbars: the default toolbars and every button
    /// shown again, the optional toolbars hidden, every bar back at its
    /// default place.
    pub(crate) fn reset_all_toolbars(&mut self) {
        self.reset_toolbar_layout();
        self.views.hidden_toolbars = default_hidden_toolbars();
        self.views.shown_toolbars.clear();
        self.views.hidden_buttons.clear();
    }

    /// View > Toolbars > `name`: show or hide one toolbar.
    pub(crate) fn toggle_toolbar(&mut self, name: &str) {
        let set = if menu::OPTIONAL_TOOLBARS.contains(&name) {
            &mut self.views.shown_toolbars
        } else {
            &mut self.views.hidden_toolbars
        };
        if !set.remove(name) {
            set.insert(name.into());
        }
    }

    /// View > Lock All Toolbars (a check item).
    pub(crate) fn lock_toolbars_item(&mut self, ui: &mut egui::Ui) {
        let locked = self.views.toolbars_locked;
        let item = Item::new("Lock All Toolbars")
            .icon(chrome::Icon::Lock)
            .checked(locked);
        if self.item(ui, "menu_lock_toolbars", item) {
            self.views.toolbars_locked = !locked;
        }
    }

    /// The Customize dialog: toolbars on the left, the selected toolbar's
    /// buttons with check boxes on the right, Reset and Close.
    pub(crate) fn customize_toolbars_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.views.customize.clone() else {
            return;
        };
        use crate::dialog_kit::{self as kit, Col, Entry};
        const W: f32 = 546.;
        const H: f32 = 400.;
        let buttons = TOOLBAR_BUTTONS[state.bar].1;
        let shown: Vec<bool> = buttons
            .iter()
            .map(|(action, _)| self.button_visible(action))
            .collect();
        let mut toggled = None;
        let (clicked, escape, actions) =
            kit::window(ctx, "Customize", "Customize", Vec2::new(W, H), |c| {
                kit::text(c, 8., 6., "Toolbars:", true);
                let out = kit::list(
                    c,
                    "customize_bars",
                    (8., 24., 160., H - 32.),
                    None,
                    &[Col {
                        title: "Toolbar",
                        width: 140.,
                    }],
                    TOOLBAR_BUTTONS.len(),
                    Some(state.bar),
                    None,
                    &mut |c, i, _, rect, sel| {
                        let bar = TOOLBAR_BUTTONS[i].0;
                        c.act(&format!("customize_bar_{bar}"), rect);
                        kit::cell_text(c, rect, bar, sel, 4.);
                    },
                );
                if let Some(i) = out.clicked {
                    state.bar = i;
                }
                kit::text(c, 176., 6., "Buttons:", true);
                kit::list(
                    c,
                    "customize_buttons",
                    (176., 24., 250., H - 32.),
                    None,
                    &[
                        Col {
                            title: "Show",
                            width: 44.,
                        },
                        Col {
                            title: "Button",
                            width: 186.,
                        },
                    ],
                    buttons.len(),
                    None,
                    None,
                    &mut |c, i, k, rect, sel| {
                        let (action, caption) = buttons[i];
                        if k == 1 {
                            kit::cell_text(c, rect, caption, sel, 4.);
                            return;
                        }
                        let key = format!("customize_button_{action}");
                        if c.hit(&key, rect, true).clicked() {
                            toggled = Some(i);
                        }
                        let at = rect.center() - c.origin - Vec2::splat(6.5);
                        kit::check_glyph(c, at.x.round(), at.y.round(), shown[i], true);
                    },
                );
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::close("customize_toolbars_close"),
                        Entry::Button("customize_toolbars_reset", "Reset", true, true),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        if let Some(i) = toggled {
            self.set_button_shown(buttons[i].0, !shown[i]);
        }
        let close = clicked.as_deref() == Some("customize_toolbars_close");
        if clicked.as_deref() == Some("customize_toolbars_reset") {
            self.reset_bar_buttons(state.bar);
        }
        if close || escape {
            self.views.customize = None;
        } else {
            self.views.customize = Some(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_toolbar_lists_its_buttons_once() {
        let bars: Vec<_> = TOOLBAR_BUTTONS.iter().map(|(bar, _)| *bar).collect();
        assert_eq!(bars, menu::BUILT_TOOLBARS);
        let mut seen = BTreeSet::new();
        for (_, buttons) in TOOLBAR_BUTTONS {
            for (action, _) in buttons {
                assert!(seen.insert(*action), "{action} listed twice");
            }
        }
        assert!(is_button("tool_tab_merge") && !is_button("menu_tab_merge"));
    }

    fn driver(name: &str) -> crate::qa_sweep::Driver {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        crate::qa_sweep::Driver::new(name, project)
    }

    fn open_toolbars_menu(d: &mut crate::qa_sweep::Driver) {
        d.click("menu_bar_view");
        assert!(d.hover_text("Toolbars"), "View > Toolbars is drawn");
    }

    #[test]
    fn customize_hides_and_resets_buttons_and_keeps_them_in_the_workspace() {
        let mut d = driver("customize-toolbars");
        assert!(d.has("tool_tab_merge") && d.has("dock_risks"));
        open_toolbars_menu(&mut d);
        d.click("menu_customize_toolbars");
        assert!(d.app.views.customize.is_some());
        d.click("customize_bar_Display");
        d.click("customize_button_tool_tab_merge");
        d.click("customize_bar_Project");
        d.click("customize_button_dock_risks");
        d.frames(2);
        assert!(!d.has("tool_tab_merge") && !d.has("dock_risks"));
        assert!(d.has("tool_tab_vertical") && d.has("dock_expenses"));
        // Reset puts back the selected toolbar's buttons only.
        d.click("customize_toolbars_reset");
        d.frames(2);
        assert!(d.has("dock_risks") && !d.has("tool_tab_merge"));
        d.click("customize_toolbars_close");
        assert!(d.app.views.customize.is_none());
        // Kept in the workspace preferences.
        d.app.save_views();
        let mut restored = views::Views::default();
        restored.restore(&d.app.store.preference("views_layout_v1").unwrap().unwrap());
        assert!(restored.hidden_buttons.contains("tool_tab_merge"));
        // Escape closes the dialog too.
        open_toolbars_menu(&mut d);
        d.click("menu_customize_toolbars");
        assert!(d.app.views.customize.is_some());
        d.key(Key::Escape);
        assert!(d.app.views.customize.is_none());
        // Reset All Toolbars shows everything again.
        d.click("menu_bar_view");
        d.click("menu_reset_toolbars");
        d.frames(2);
        assert!(d.has("tool_tab_merge"));
        assert!(d.app.views.hidden_buttons.is_empty());
        d.assert_clean("customize toolbars");
    }

    /// The buttons of toolbar `bar`.
    fn buttons(bar: &str) -> &'static [(&'static str, &'static str)] {
        TOOLBAR_BUTTONS
            .iter()
            .find(|(name, _)| *name == bar)
            .map(|(_, buttons)| *buttons)
            .unwrap()
    }

    /// Switch toolbar `bar` on or off through View > Toolbars.
    fn toggle_bar(d: &mut crate::qa_sweep::Driver, bar: &str) {
        open_toolbars_menu(d);
        d.click(&format!("menu_toolbar_{bar}"));
        d.frames(2);
    }

    #[test]
    fn optional_toolbars_start_hidden_and_open_from_view_toolbars() {
        let mut d = driver("optional-toolbars");
        for bar in menu::OPTIONAL_TOOLBARS {
            assert!(!d.app.toolbar_visible(bar), "{bar} starts hidden");
            for (action, _) in buttons(bar) {
                assert!(!d.has(action), "{action} hidden with its toolbar");
            }
            toggle_bar(&mut d, bar);
            assert!(d.app.toolbar_visible(bar), "{bar} shown");
            for (action, _) in buttons(bar) {
                let r = d.rect(action);
                assert!(
                    r.top() >= menu::MENU_ROW + menu::TOOLBAR_ROW
                        && r.bottom() <= menu::MENU_ROW + 2. * menu::TOOLBAR_ROW,
                    "{action} sits on the second toolbar row: {r:?}"
                );
            }
        }
        // Kept in the workspace preferences, and off again from the menu.
        d.app.save_views();
        let mut restored = views::Views::default();
        restored.restore(&d.app.store.preference("views_layout_v1").unwrap().unwrap());
        assert_eq!(restored.shown_toolbars.len(), menu::OPTIONAL_TOOLBARS.len());
        toggle_bar(&mut d, "Find");
        assert!(!d.has("tool_find") && d.has("tool_run_report"));
        d.assert_clean("optional toolbars");
    }

    #[test]
    fn every_optional_toolbar_button_runs_its_command() {
        let mut d = driver("optional-toolbar-buttons");
        for bar in menu::OPTIONAL_TOOLBARS {
            toggle_bar(&mut d, bar);
        }
        d.home();
        let baseline = d.keys();
        // Buttons that open a dialog: each opens one and Cancel or Close
        // leaves the project as it was.
        for action in [
            "tool_apply_actuals",
            "tool_store_period",
            "tool_recalc_costs",
            "tool_global_change",
            "tool_admin_preferences",
            "tool_admin_categories",
            "tool_currencies",
            "tool_fin_calendars",
            "tool_find",
            "tool_replace",
            "tool_run_batch",
            "tool_import_report",
            "tool_report_groups",
            "tool_batch_reports",
            "tool_report_wizard",
            "tool_check_schedule",
        ] {
            d.home();
            let pressed = d.try_command(action, &baseline, "optional toolbar");
            assert!(!pressed.is_empty(), "{action} opened no dialog");
        }
        // Summarize (it only posted a status line) was removed.
        d.home();
        assert!(!d.has("tool_summarize"));
        // The check filters dropdown lists the View > Check Schedule filters
        // and applies one.
        d.home();
        d.click("tool_check_filters");
        d.frames(2);
        let first = layout_opts::checks::CheckFilter::ALL[0];
        let item = format!("tool_{}", first.key().replace(':', "_"));
        assert!(d.has(&item), "the dropdown lists {item}");
        d.click(&item);
        d.frames(2);
        assert!(d.app.check_filter_on(first));
        d.click("tool_check_filters");
        d.click(&item);
        d.frames(2);
        assert!(!d.app.check_filter_on(first));
        // Find Next needs a search first.
        d.home();
        assert!(d.has("tool_find_next"));
        d.click("tool_find_next");
        assert!(
            d.keys().iter().all(|k| baseline.contains(k)),
            "tool_find_next"
        );
        d.assert_clean("optional toolbar buttons");
    }

    #[test]
    fn customize_and_reset_cover_the_optional_toolbars() {
        let mut d = driver("customize-optional");
        toggle_bar(&mut d, "Reports");
        toggle_bar(&mut d, "Check Schedule");
        assert!(d.has("tool_report_wizard") && d.has("tool_check_filters"));
        open_toolbars_menu(&mut d);
        d.click("menu_customize_toolbars");
        d.click("customize_bar_Reports");
        d.click("customize_button_tool_report_wizard");
        d.click("customize_bar_Check Schedule");
        d.click("customize_button_tool_check_filters");
        d.frames(2);
        assert!(!d.has("tool_report_wizard") && !d.has("tool_check_filters"));
        assert!(d.has("tool_run_report") && d.has("tool_check_schedule"));
        // Reset puts back the selected toolbar's buttons.
        d.click("customize_toolbars_reset");
        d.frames(2);
        assert!(d.has("tool_check_filters") && !d.has("tool_report_wizard"));
        d.click("customize_toolbars_close");
        // A hidden button of a hidden toolbar stays hidden when the toolbar
        // comes back.
        toggle_bar(&mut d, "Reports");
        toggle_bar(&mut d, "Reports");
        assert!(d.has("tool_run_report") && !d.has("tool_report_wizard"));
        // Reset All Toolbars: optional toolbars hidden again, every button
        // shown.
        d.click("menu_bar_view");
        d.click("menu_reset_toolbars");
        d.frames(2);
        assert!(d.app.views.shown_toolbars.is_empty() && d.app.views.hidden_buttons.is_empty());
        assert!(!d.has("tool_run_report") && !d.has("tool_check_schedule"));
        assert!(d.has("tool_tab_merge"));
        d.assert_clean("customize optional toolbars");
    }

    /// Opens bar `bar`'s chevron menu and its Add or Remove Buttons list.
    fn open_add_remove(d: &mut crate::qa_sweep::Driver, chevron: &str) {
        d.click(chevron);
        assert!(d.has("chevron_customize"), "{chevron} opens its menu");
        assert!(
            d.hover_text("Add or Remove Buttons"),
            "{chevron}: Add or Remove Buttons is drawn"
        );
    }

    #[test]
    fn chevron_menu_adds_and_removes_buttons_with_the_customize_state() {
        let mut d = driver("chevron-add-remove");
        assert!(d.has("bar_chevron_Display") && d.has("tool_tab_merge"));
        open_add_remove(&mut d, "bar_chevron_Display");
        for (action, _) in buttons("Display") {
            assert!(
                d.has(&format!("chevron_button_{action}")),
                "{action} listed"
            );
        }
        // Off: the bar drops the button at once and the list stays open.
        d.click("chevron_button_tool_tab_merge");
        assert!(!d.app.button_visible("tool_tab_merge"));
        assert!(!d.has("tool_tab_merge") && d.has("tool_tab_vertical"));
        assert!(
            d.has("chevron_button_tool_tab_merge"),
            "the list stays open"
        );
        // And on again from the same open list.
        d.click("chevron_button_tool_tab_merge");
        assert!(d.app.button_visible("tool_tab_merge") && d.has("tool_tab_merge"));
        d.click("chevron_button_tool_tab_merge");
        assert!(!d.has("tool_tab_merge"));
        // Escape closes the menu.
        d.key(Key::Escape);
        d.frames(2);
        assert!(!d.has("chevron_customize") && !d.has("chevron_button_tool_tab_merge"));
        // Customize... opens the dialog on this bar, which shows the same
        // state: its check box shows the button again.
        d.click("bar_chevron_Display");
        d.click("chevron_customize");
        assert_eq!(
            d.app.views.customize,
            Some(Customize {
                bar: bar_index("Display").unwrap()
            })
        );
        assert!(!d.has("chevron_customize"), "the menu closes");
        d.click("customize_button_tool_tab_merge");
        assert!(d.has("tool_tab_merge"));
        d.click("customize_toolbars_close");
        // The chevron reads it back: its click hides the button again.
        open_add_remove(&mut d, "bar_chevron_Display");
        d.click("chevron_button_tool_tab_merge");
        assert!(!d.app.button_visible("tool_tab_merge"));
        d.key(Key::Escape);
        // A hidden button's command stays in the menus.
        d.click("menu_bar_view");
        assert!(d.hover_text("Tab Groups"));
        assert!(d.has("menu_tab_merge"), "View > Tab Groups still merges");
        d.key(Key::Escape);
        d.key(Key::Escape);
        // Kept with the user's preferences, as Customize keeps it.
        d.app.save_views();
        let mut restored = views::Views::default();
        restored.restore(&d.app.store.preference("views_layout_v1").unwrap().unwrap());
        assert!(restored.hidden_buttons.contains("tool_tab_merge"));
        d.assert_clean("chevron add or remove");
    }

    #[test]
    fn chevron_reset_toolbar_restores_only_that_bar() {
        let mut d = driver("chevron-reset");
        d.app.set_button_shown("tool_tab_merge", false);
        d.app.set_button_shown("tool_whats_new", false);
        d.app.set_button_shown("dock_risks", false);
        d.frames(2);
        d.click("bar_chevron_Display");
        d.click("chevron_reset");
        assert!(d.app.button_visible("tool_tab_merge") && d.app.button_visible("tool_whats_new"));
        assert!(
            !d.app.button_visible("dock_risks"),
            "the Project bar keeps its change"
        );
        assert!(d.has("tool_tab_merge") && !d.has("dock_risks"));
        assert!(!d.has("chevron_reset"), "the menu closes");
        // The left dock's Project bar resets from its own chevron.
        d.click("bar_chevron_Project");
        d.click("chevron_reset");
        assert!(d.has("dock_risks") && d.app.views.hidden_buttons.is_empty());
        d.assert_clean("chevron reset");
    }

    #[test]
    fn chevron_menu_toggles_and_closes_on_a_click_elsewhere() {
        let mut d = driver("chevron-close");
        d.click("bar_chevron_Layout");
        assert!(d.has("chevron_customize"));
        // A click on the chevron again closes it.
        d.click("bar_chevron_Layout");
        assert!(!d.has("chevron_customize"));
        // So does a click outside it.
        d.click("bar_chevron_Layout");
        assert!(d.has("chevron_customize"));
        d.click_at(Pos2::new(1200., 600.));
        d.frames(2);
        assert!(!d.has("chevron_customize"));
        // The chevron of another bar opens that bar's menu.
        open_add_remove(&mut d, "bar_chevron_Print");
        assert!(d.has("chevron_button_tool_print") && !d.has("chevron_button_tool_bars"));
        d.key(Key::Escape);
        d.assert_clean("chevron close");
    }

    #[test]
    fn every_bar_keeps_a_working_chevron_with_every_button_hidden() {
        let mut d = driver("chevron-empty");
        // A top bar, a left and a right dock bar.
        for bar in ["Display", "Standard", "Edit"] {
            for (action, _) in buttons(bar) {
                d.app.set_button_shown(action, false);
            }
        }
        d.frames(3);
        for bar in ["Display", "Standard", "Edit"] {
            let chevron = format!("bar_chevron_{bar}");
            assert!(d.has(&chevron), "{bar} keeps its chevron");
            assert!(d.has(&format!("bar_grip_{bar}")), "{bar} keeps its grip");
            for (action, _) in buttons(bar) {
                assert!(!d.has(action), "{action} hidden");
            }
            // The first button comes back from the chevron.
            let first = buttons(bar)[0].0;
            open_add_remove(&mut d, &chevron);
            d.click(&format!("chevron_button_{first}"));
            assert!(d.has(first), "{first} back on {bar}");
            d.key(Key::Escape);
            d.click(&chevron);
            d.click("chevron_reset");
            for (action, _) in buttons(bar) {
                assert!(d.has(action), "{action} back after Reset Toolbar");
            }
        }
        d.assert_clean("chevron empty bars");
    }

    #[test]
    fn chevrons_work_locked_floating_on_the_bottom_dock_and_on_the_menu_bar() {
        let mut d = driver("chevron-places");
        // Locked toolbars cannot move, but their buttons can still change.
        d.app.views.toolbars_locked = true;
        d.frames(2);
        open_add_remove(&mut d, "bar_chevron_Display");
        d.click("chevron_button_fit");
        assert!(!d.has("fit"));
        d.key(Key::Escape);
        d.app.views.toolbars_locked = false;
        // A floating bar has its chevron in its title strip.
        d.app
            .views
            .bars
            .layout
            .float("Layout", Pos2::new(500., 300.), 200.);
        d.frames(3);
        let title = d.app.views.bars.rt.geo.floats["Layout"];
        let chevron = d.rect("float_chevron_Layout");
        assert!(title.contains_rect(chevron), "{chevron:?} in {title:?}");
        open_add_remove(&mut d, "float_chevron_Layout");
        d.click("chevron_button_tool_columns");
        assert!(!d.has("tool_columns") && d.has("tool_bars"));
        d.key(Key::Escape);
        d.app.views.bars.layout.redock("Layout");
        // A bar on the bottom dock opens its menu upward.
        d.app.views.bars.layout.apply(
            "Print",
            crate::dockbars::Drop::NewRow {
                side: crate::dockbars::Side::Bottom,
                row: 0,
            },
        );
        d.frames(3);
        let chevron = d.rect("bar_chevron_Print");
        d.click("bar_chevron_Print");
        let customize = d.rect("chevron_customize");
        assert!(
            customize.bottom() <= chevron.top() + 1.,
            "{customize:?} over {chevron:?}"
        );
        d.key(Key::Escape);
        // The menu bar's glyph: Customize... works, it has no buttons to list.
        d.click("bar_chevron_Menu Bar");
        assert!(d.has("chevron_add_remove") && d.has("chevron_reset"));
        d.click("chevron_reset");
        assert!(
            d.has("chevron_customize"),
            "Reset Toolbar is disabled there"
        );
        d.click("chevron_customize");
        assert_eq!(d.app.views.customize, Some(Customize { bar: 0 }));
        d.key(Key::Escape);
        d.assert_clean("chevron places");
    }

    #[test]
    fn button_icons_match_what_the_bars_draw() {
        let mut d = driver("chevron-icons");
        for bar in menu::OPTIONAL_TOOLBARS {
            toggle_bar(&mut d, bar);
        }
        d.frames(2);
        for (bar, buttons) in TOOLBAR_BUTTONS {
            for (action, _) in buttons {
                let drawn = d.app.views.bars.rt.drawn_icons.get(*action).copied();
                assert!(drawn.is_some(), "{bar}: {action} was not drawn");
                assert_eq!(button_icon(action), drawn, "{bar}: {action}");
            }
        }
    }

    #[test]
    fn lock_all_toolbars_toggles_and_is_kept() {
        let mut d = driver("lock-toolbars");
        assert!(!d.app.views.toolbars_locked);
        // Grip dots in the grip columns of the first toolbar row.
        let grip_ink = |d: &mut crate::qa_sweep::Driver| {
            let top = menu::MENU_ROW;
            let area = Rect::from_min_max(Pos2::new(2., top + 1.), Pos2::new(9., top + 27.));
            d.frame(vec![])
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .flat_map(|mesh| mesh.vertices.iter())
                .filter(|v| v.color == chrome::pal::GRIP && area.contains(v.pos))
                .count()
        };
        assert!(grip_ink(&mut d) > 5, "the grip is drawn");
        d.click("menu_bar_view");
        d.click("menu_lock_toolbars");
        assert!(d.app.views.toolbars_locked);
        assert_eq!(grip_ink(&mut d), 0, "locked toolbars have no grip");
        // The buttons stay where they were.
        assert!(d.has("tool_tab_merge") && d.has("dock_risks"));
        d.app.save_views();
        let mut restored = views::Views::default();
        restored.restore(&d.app.store.preference("views_layout_v1").unwrap().unwrap());
        assert!(restored.toolbars_locked);
        d.click("menu_bar_view");
        d.click("menu_lock_toolbars");
        assert!(!d.app.views.toolbars_locked);
    }
}

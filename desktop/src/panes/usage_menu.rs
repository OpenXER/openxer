//! Right-click menus of the usage spreadsheets and profiles, built on the
//! shared context-menu helper. A menu is built fresh each time it opens, so
//! checkmarks and enabled states always come from live state.
use super::stacked::StackBy;
use super::time_fields::Sheet;
use super::usage_cells::SheetHost;
use super::usage_fields::FieldsView;
use super::*;
use crate::context_menu::{ContextMenu, Entry, Menu, Spec};
use chrome::{Icon, Item};

/// The Resource Usage Spreadsheet's Spreadsheet Fields submenu: item key,
/// field id, and the field's name in units and in costs
/// (`TimeField::name`; a test keeps them the same).
pub(crate) const QUICK_FIELDS: [(&str, &str, &str, &str); 3] = [
    (
        "fields_remaining",
        "remaining",
        "Remaining Early Units",
        "Remaining Early Cost",
    ),
    ("fields_actual", "actual", "Actual Units", "Actual Cost"),
    (
        "fields_planned",
        "planned",
        "Budgeted Units",
        "Budgeted Cost",
    ),
];

/// Which usage view a menu belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UsageMenu {
    ActivitySheet,
    ResourceSheet,
    ActivityProfile,
    ResourceProfile,
}

impl UsageMenu {
    pub fn is_profile(self) -> bool {
        matches!(
            self,
            UsageMenu::ActivityProfile | UsageMenu::ResourceProfile
        )
    }

    /// The prefix of the item rectangles, `<prefix>_<item>`.
    pub fn prefix(self) -> &'static str {
        if self.is_profile() {
            "usage_profile_menu"
        } else {
            "usage_sheet_menu"
        }
    }

    fn fields_view(self) -> FieldsView {
        match self {
            UsageMenu::ActivitySheet | UsageMenu::ActivityProfile => FieldsView::ActivitySheet,
            UsageMenu::ResourceSheet => FieldsView::ResourceSheet,
            UsageMenu::ResourceProfile => FieldsView::ResourceProfile,
        }
    }
}

impl App {
    /// Opens `kind`'s menu on a right-click of `response`.
    pub(crate) fn usage_menu(&mut self, response: &egui::Response, kind: UsageMenu) {
        ContextMenu::new(kind.prefix())
            .min_width(190.)
            .show(response, self, |app, m| app.usage_menu_items(m, kind));
    }

    pub(super) fn usage_menu_items(&mut self, m: &mut Menu, kind: UsageMenu) {
        if kind == UsageMenu::ResourceSheet {
            self.resource_sheet_items(m);
        } else if kind.is_profile() {
            self.usage_profile_items(m, kind);
        } else {
            self.timescale_item(m);
            self.usage_sheet_items(m, kind);
        }
        m.separator();
        m.submenu("display", Item::new("Display"), |m| {
            self.usage_display_menu(m.ui());
        });
    }

    fn timescale_item(&mut self, m: &mut Menu) {
        let timescale = Item::new("Timescale...").icon(Icon::Timescale);
        if m.item("timescale", timescale) {
            self.open_timescale_dialog();
        }
    }

    /// The Resource Usage Spreadsheet's menu in the reference's order: Show
    /// All Projects; Timescale..., Spreadsheet Options..., User
    /// Preferences...; Zoom In and Out; the cell commands; Spreadsheet
    /// Fields, a submenu of three fields and Customize.... Copy to Excel
    /// and Display follow, OpenXER's own.
    fn resource_sheet_items(&mut self, m: &mut Menu) {
        let entries = self.resource_sheet_entries();
        let ctx = m.ui().ctx().clone();
        if let Some(key) = m.entries(&entries) {
            self.resource_sheet_command(key, &ctx);
        }
        if m.item("copy_to_excel", Item::new("Copy to Excel")) {
            self.copy_usage_to_excel();
        }
    }

    /// The reference's part of the Resource Usage Spreadsheet's menu, as
    /// data: the same menu on the Resource Assignments window's
    /// spreadsheet. Its commands act on the spreadsheet that holds the
    /// cell selection (`usage_cells::SheetHost`).
    pub(crate) fn resource_sheet_entries(&self) -> Vec<Entry> {
        let window = self.panes.sheet_host == SheetHost::Assignments;
        let (all, costs) = if window {
            (self.views.assignments.show_all_projects, false)
        } else {
            (self.panes.usage.show_all_projects, self.panes.usage.costs)
        };
        let selected = self
            .panes
            .sheet_cells
            .range(&self.panes.sheet_periods)
            .is_some();
        let copied = self.panes.sheet_cells.copied.is_some();
        let mut fields: Vec<Entry> = QUICK_FIELDS
            .iter()
            .map(|&(key, _, units, cost)| Spec::new(key, if costs { cost } else { units }).into())
            .collect();
        fields.push(Spec::new("time_fields_customize", "Customize...").into());
        vec![
            Spec::new("show_all_projects", "Show All Projects")
                .checked(all)
                .into(),
            Entry::Separator,
            Spec::new("timescale", "Timescale...")
                .icon(Icon::Timescale)
                .into(),
            Spec::new("options", "Spreadsheet Options...")
                .icon(Icon::Options)
                .into(),
            Spec::new("user_preferences", "User Preferences...")
                .icon(Icon::Preferences)
                .into(),
            Entry::Separator,
            Spec::new("zoom_in", "Zoom In").icon(Icon::ZoomIn).into(),
            Spec::new("zoom_out", "Zoom Out").icon(Icon::ZoomOut).into(),
            Entry::Separator,
            Spec::new("fill_down", "Fill Down")
                .key("Ctrl+E")
                .icon(Icon::FillDown)
                .enabled(self.sheet_fill_ready(true))
                .into(),
            Spec::new("fill_across", "Fill Across")
                .key("Ctrl+H")
                .enabled(self.sheet_fill_ready(false))
                .into(),
            Spec::new("range_copy", "Range Copy")
                .key("Shift+Ctrl+C")
                .icon(Icon::Copy)
                .enabled(selected)
                .into(),
            Spec::new("range_paste", "Range Paste")
                .key("Shift+Ctrl+V")
                .icon(Icon::Paste)
                .enabled(selected && copied)
                .into(),
            Entry::Separator,
            // Three fields to show alone, then Customize...; none is checked.
            Entry::Submenu(
                Spec::new("spreadsheet_fields", "Spreadsheet Fields"),
                fields,
            ),
        ]
    }

    /// A command of `resource_sheet_entries`, on the spreadsheet that holds
    /// the cell selection.
    pub(crate) fn resource_sheet_command(&mut self, key: &str, ctx: &egui::Context) {
        let window = self.panes.sheet_host == SheetHost::Assignments;
        let time = ctx.input(|i| i.time);
        match key {
            "show_all_projects" if window => {
                let s = &mut self.views.assignments;
                s.show_all_projects = !s.show_all_projects;
            }
            "show_all_projects" => {
                let u = &mut self.panes.usage;
                u.show_all_projects = !u.show_all_projects;
            }
            "timescale" => self.open_timescale_dialog(),
            // The window's own Spreadsheet Fields: its table's columns.
            "options" if window => self.open_spreadsheet_fields(FieldsView::Assignments),
            "options" => self.open_spreadsheet_fields(UsageMenu::ResourceSheet.fields_view()),
            "user_preferences" => self.open_user_preferences(),
            "zoom_in" | "zoom_out" if window => {
                self.views.assignments.zoom_step(key == "zoom_in");
            }
            "zoom_in" | "zoom_out" => self.zoom_ladder(key == "zoom_in"),
            "fill_down" => self.sheet_fill(true, time),
            "fill_across" => self.sheet_fill(false, time),
            "range_copy" => self.sheet_range_copy(ctx),
            "range_paste" => self.sheet_range_paste(time),
            "time_fields_customize" => self.open_time_fields(Sheet::Resource),
            _ => {
                if let Some((_, id, _, _)) = QUICK_FIELDS.iter().find(|f| f.0 == key) {
                    let ids = vec![id.to_string()];
                    if window {
                        self.views.assignments.time_field_ids = ids;
                    } else {
                        self.panes.usage.resource_time_fields = ids;
                    }
                }
            }
        }
    }

    fn show_all_projects_item(&mut self, m: &mut Menu) {
        let on = self.panes.usage.show_all_projects;
        if m.check("show_all_projects", "Show All Projects", on) {
            self.panes.usage.show_all_projects = !on;
        }
    }

    fn zoom_items(&mut self, m: &mut Menu) {
        if m.item("zoom_in", Item::new("Zoom In").icon(Icon::ZoomIn)) {
            self.zoom_ladder(true);
        }
        if m.item("zoom_out", Item::new("Zoom Out").icon(Icon::ZoomOut)) {
            self.zoom_ladder(false);
        }
    }

    fn usage_sheet_items(&mut self, m: &mut Menu, kind: UsageMenu) {
        let options = Item::new("Spreadsheet Options...").icon(Icon::Options);
        if m.item("options", options) {
            self.open_spreadsheet_fields(kind.fields_view());
        }
        self.show_all_projects_item(m);
        m.separator();
        self.zoom_items(m);
        m.separator();
        let prefs = Item::new("User Preferences...").icon(Icon::Preferences);
        if m.item("user_preferences", prefs) {
            self.open_user_preferences();
        }
        if m.item("copy_to_excel", Item::new("Copy to Excel")) {
            self.copy_usage_to_excel();
        }
        m.separator();
        // The time-phased fields the spreadsheet shows, one line each: on
        // this spreadsheet the item opens the Fields dialog itself.
        if m.item("spreadsheet_fields", Item::new("Spreadsheet Fields...")) {
            self.open_time_fields(Sheet::Activity);
        }
    }

    /// The profiles' menu in the reference's order: Show All Projects;
    /// Stacked Histogram; Timescale..., Profile Options..., User
    /// Preferences...; Zoom In and Out. OpenXER's own follow: Stack By, Show
    /// Actual and Remaining, Show Usage for (and Display, added by the
    /// caller).
    fn usage_profile_items(&mut self, m: &mut Menu, kind: UsageMenu) {
        let resource = kind == UsageMenu::ResourceProfile;
        self.show_all_projects_item(m);
        m.separator();
        // Stacked Histogram: the bars split by resource or by role.
        let on = if resource {
            self.panes.usage.stack_resource
        } else {
            self.panes.usage.stack_activity
        };
        if m.check("stacked", "Stacked Histogram", on) {
            if resource {
                self.panes.usage.stack_resource = !on;
            } else {
                self.panes.usage.stack_activity = !on;
            }
        }
        m.separator();
        self.timescale_item(m);
        let options = Item::new("Profile Options...").icon(Icon::Options);
        if m.item("options", options) {
            self.open_profile_options();
        }
        let prefs = Item::new("User Preferences...").icon(Icon::Preferences);
        if m.item("user_preferences", prefs) {
            self.open_user_preferences();
        }
        m.separator();
        self.zoom_items(m);
        m.separator();
        m.submenu("stack_by", Item::new("Stack By"), |m| {
            let by = self.panes.usage.stack_by;
            for (key, label, choice) in [
                ("stack_by_resource", "Resource", StackBy::Resource),
                ("stack_by_role", "Role", StackBy::Role),
            ] {
                if m.item(key, Item::new(label).checked(by == choice)) {
                    self.panes.usage.stack_by = choice;
                    // Picking a split turns stacking on.
                    if resource {
                        self.panes.usage.stack_resource = true;
                    } else {
                        self.panes.usage.stack_activity = true;
                    }
                }
            }
        });
        // Show Actual and Remaining: actual and remaining in one bar (on) or
        // side by side; with Stacked Histogram, each series split in two.
        let both = if resource {
            self.panes.usage.stacked_resource
        } else {
            self.panes.usage.stacked_activity
        };
        if m.check("actual_remaining", "Show Actual and Remaining", both) {
            if resource {
                self.panes.usage.stacked_resource = !both;
            } else {
                self.panes.usage.stacked_activity = !both;
            }
        }
        m.submenu("show_usage_for", Item::new("Show Usage for"), |m| {
            let selected = self.panes.usage.selected_only;
            let activity = !resource;
            let all = Item::new("All activities")
                .checked(!selected)
                .enabled(activity);
            if m.item("all_activities", all) {
                self.panes.usage.selected_only = false;
            }
            let chosen = Item::new("Selected activities")
                .checked(selected)
                .enabled(activity);
            if m.item("selected_activities", chosen) {
                self.panes.usage.selected_only = true;
            }
            m.separator();
            let by_resource = Item::new("Resource")
                .checked(self.panes.usage.resource_filter)
                .enabled(resource);
            if m.item("resource", by_resource) {
                self.panes.usage.resource_filter = !self.panes.usage.resource_filter;
            }
            let by_period = Item::new("Time Period")
                .checked(self.panes.usage.time_period)
                .enabled(activity);
            if m.item("time_period", by_period) {
                self.panes.usage.time_period = !self.panes.usage.time_period;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::time_fields::{Sheet, TimeField};
    use super::QUICK_FIELDS;

    #[test]
    fn quick_fields_carry_the_field_names() {
        for (_, id, units, costs) in QUICK_FIELDS {
            let f = TimeField::parse(Sheet::Resource, id).unwrap();
            assert_eq!(f.name(Sheet::Resource, false), units);
            assert_eq!(f.name(Sheet::Resource, true), costs);
        }
    }
}

//! OpenXER's own help: short topics compiled into the app, one per dialog
//! and per main view, plus a few overview pages. The text lives in
//! `desktop/help/*.md` (a small Markdown subset, see [`parse`]) and the
//! table of contents in `desktop/help/contents.md`.
//!
//! Every dialog's Help button calls [`App::open_help_for`] with the dialog's
//! name (the salt or title it notes with `dialog_kit::note_client`), and F1
//! looks up the topmost open dialog the same way, so a Help button and F1 on
//! one dialog always land on the same topic. With no dialog open F1 opens
//! the current view's topic. Ctrl+F1 and Help > Help Topics... open the
//! overview.
use super::*;
use std::sync::OnceLock;

mod viewer;

#[cfg(test)]
mod tests;

/// The topic Ctrl+F1 and Help > Help Topics... open.
pub(crate) const OVERVIEW: &str = "overview";

/// The help window's title, which is also the name it notes as a dialog.
pub(crate) const WINDOW_TITLE: &str = "OpenXER Help";

/// Topic files, by id (the file stem in `desktop/help/`).
macro_rules! topics {
    ($($id:literal),* $(,)?) => {
        &[$(($id, include_str!(concat!("../../help/", $id, ".md")))),*]
    };
}

const SOURCES: &[(&str, &str)] = topics![
    // Overview
    "overview",
    "getting-started",
    "scheduling-basics",
    "working-with-ai-agents",
    "ai-chat",
    "skills-for-ai-agents",
    "keyboard-shortcuts",
    // Views
    "view-activities",
    "view-wbs",
    "view-projects",
    "view-resources",
    "view-assignments",
    "view-reports",
    "view-expenses",
    "view-documents",
    "view-risks",
    "view-issues",
    "view-thresholds",
    "view-report-tab",
    // Files and projects
    "new-project",
    "open-project",
    "default-project",
    "import-wizard",
    "export-wizard",
    "page-setup",
    "print-preview",
    // Editing
    "new-activity-wizard",
    "new-resource-wizard",
    "edit-relationship",
    "assign-relationships",
    "assign-dialogs",
    "notebooks",
    "find-and-replace",
    "renumber-activity-ids",
    "copy-options",
    "global-change",
    "global-change-edit",
    "spell-check",
    "text-attachment",
    // Layouts and display
    "customize-activity-details",
    "columns",
    "filters",
    "filter-editor",
    "group-and-sort",
    "open-layout",
    "new-layout",
    "table-font-and-row",
    "chart-font-and-colors",
    "collapse-to",
    "bars",
    "bar-chart-options",
    "timescale",
    "activity-network-options",
    "trace-logic-options",
    "usage-views",
    "profile-options",
    // Scheduling and progress
    "schedule",
    "schedule-options",
    "check-schedule",
    "level-resources",
    "update-progress",
    "apply-actuals",
    "recalculate-assignment-costs",
    "save-period-actuals",
    "spread-budget-hours",
    // Baselines and comparison
    "maintain-baselines",
    "assign-baselines",
    "add-baseline",
    "select-baseline-project",
    "update-baseline",
    "revert-to-baseline",
    "schedule-comparison",
    "schedule-comparison-results",
    // Calendars
    "calendars",
    "calendar-editor",
    "calendar-weekly-hours",
    "financial-period-calendars",
    // Dictionaries and enterprise
    "project-folders",
    "responsible-managers",
    "roles",
    "role-codes",
    "resource-shifts",
    "resource-curves",
    "merge-resources",
    "shared-dictionaries",
    "funding-sources",
    "currencies",
    "codes",
    "user-defined-fields",
    "step-templates",
    "cost-accounts",
    "categories-and-lists",
    // Reports and checks
    "report-groups",
    "batch-reports",
    "run-batch",
    "s-curve",
    "earned-hours",
    "run-report",
    "report-editor",
    "report-properties",
    "import-reports",
    "import-reports-database",
    "export-report",
    "project-checker",
    "project-checker-results",
    "monitor-thresholds",
    "issue-navigator",
    // Preferences and tools
    "user-preferences",
    "admin-preferences",
    "customize-toolbars",
    "external-applications",
];

const CONTENTS: &str = include_str!("../../help/contents.md");

/// The release notes, `CHANGELOG.md` at the repository root, shown as the
/// topic Help > What's New... opens.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// The topic id of the release notes.
pub(crate) const WHATS_NEW: &str = "whats-new";

/// The release notes as a help topic: each release (`## [x.y.z] - date`, the
/// newest first, as the changelog keeps them) a heading, each kind of
/// change (`### Added`) a bold line, and the changes bullets. Web links
/// keep their text only, and the changelog's own preamble and link
/// definitions are left out.
pub(crate) fn whats_new_source(changelog: &str) -> String {
    let mut out = String::from(
        "# What's New\n\nThe changes in each release of OpenXER, the newest release first.\n\n",
    );
    let mut started = false;
    for line in changelog.lines() {
        let trimmed = line.trim_end();
        if let Some(release) = trimmed.strip_prefix("## ") {
            started = true;
            let name = release.replacen('[', "", 1).replacen(']', "", 1);
            out.push_str(&format!("## {}\n", name.trim()));
            continue;
        }
        if !started {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.contains("]: ") {
            continue;
        }
        let text = plain_links(trimmed);
        if let Some(kind) = text.strip_prefix("### ") {
            out.push_str(&format!("\n**{}**\n\n", kind.trim()));
        } else {
            out.push_str(&text);
            out.push('\n');
        }
    }
    out
}

/// `[text](address)` written as its text when the address is not a topic.
fn plain_links(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(mid) = after.find("](") else {
            break;
        };
        let Some(end) = after[mid + 2..].find(')') else {
            break;
        };
        let target = &after[mid + 2..mid + 2 + end];
        out.push_str(&rest[..open]);
        if target.contains(':') || target.contains('/') || target.contains('#') {
            out.push_str(&after[..mid]);
        } else {
            out.push_str(&rest[open..open + 1 + mid + 2 + end + 1]);
        }
        rest = &after[mid + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Dialog names (as noted by the dialog kit) and the topic each opens.
/// A Help button passes its dialog's name; F1 passes the topmost dialog's.
pub(crate) const CONTEXTS: &[(&str, &str)] = &[
    ("open_project", "open-project"),
    ("default_project", "default-project"),
    ("page_setup", "page-setup"),
    ("edit_relationship", "edit-relationship"),
    ("renumber_ids", "renumber-activity-ids"),
    ("copy_activity_options", "copy-options"),
    ("renumber_duplicates", "renumber-activity-ids"),
    ("global_change_list", "global-change"),
    ("global_change_report", "global-change"),
    ("Modify Global Change", "global-change-edit"),
    ("spell", "spell-check"),
    ("text_attachment", "text-attachment"),
    ("grid_columns", "columns"),
    ("grid_filters", "filters"),
    ("grid_filter_editor", "filter-editor"),
    ("grid_group_sort", "group-and-sort"),
    ("grid_sort", "group-and-sort"),
    ("grid_open_layout", "open-layout"),
    ("new_layout", "new-layout"),
    ("table_font", "table-font-and-row"),
    ("chart_font", "chart-font-and-colors"),
    ("collapse_to", "collapse-to"),
    ("bars", "bars"),
    ("chart", "bar-chart-options"),
    ("ts", "timescale"),
    ("Activity Network Options", "activity-network-options"),
    ("Trace Logic Options", "trace-logic-options"),
    ("usage_profile_options", "profile-options"),
    ("usage_fields", "usage-views"),
    ("usage_time_fields", "usage-views"),
    ("usage_list_filter", "usage-views"),
    ("usage_list_group", "usage-views"),
    ("schedule", "schedule"),
    ("options", "schedule-options"),
    ("check", "check-schedule"),
    ("level", "level-resources"),
    ("update", "update-progress"),
    ("actuals", "apply-actuals"),
    ("recalc", "recalculate-assignment-costs"),
    ("store_period", "save-period-actuals"),
    ("top_down", "spread-budget-hours"),
    ("baselines_maintain", "maintain-baselines"),
    ("baselines_assign", "assign-baselines"),
    ("baselines_add", "add-baseline"),
    ("baselines_select", "select-baseline-project"),
    ("baselines_update", "update-baseline"),
    ("baselines_lookup", "assign-baselines"),
    ("baselines_revert", "revert-to-baseline"),
    ("compare", "schedule-comparison"),
    ("compare_results", "schedule-comparison-results"),
    ("calendars", "calendars"),
    ("calendar_editor", "calendar-editor"),
    ("calendar_workweek", "calendar-weekly-hours"),
    ("fin_calendars", "financial-period-calendars"),
    ("eps", "project-folders"),
    ("obs", "responsible-managers"),
    ("roles", "roles"),
    ("role_codes", "role-codes"),
    ("role_code_types", "role-codes"),
    ("shifts", "resource-shifts"),
    ("curves", "resource-curves"),
    ("curve_editor", "resource-curves"),
    ("funding", "funding-sources"),
    ("currencies", "currencies"),
    ("Activity Codes", "codes"),
    ("Project Codes", "codes"),
    ("Resource Codes", "codes"),
    ("Assignment Codes", "codes"),
    ("User Defined Fields", "user-defined-fields"),
    ("Activity Step Templates", "step-templates"),
    ("Cost Accounts", "cost-accounts"),
    ("Categories and Lists", "categories-and-lists"),
    ("report_groups", "report-groups"),
    ("batch_reports", "batch-reports"),
    ("run_batch", "run-batch"),
    ("scurve", "s-curve"),
    ("earned_hours", "earned-hours"),
    ("run_report", "run-report"),
    ("report_editor", "report-editor"),
    ("report_properties", "report-properties"),
    ("import_reports", "import-reports"),
    ("import_database", "import-reports-database"),
    ("export_report", "export-report"),
    ("checker", "project-checker"),
    ("checker_results", "project-checker-results"),
    ("monitor_thresholds", "monitor-thresholds"),
    (crate::views::resource_merge::SALT, "merge-resources"),
    (crate::share_dictionaries::SALT, "shared-dictionaries"),
    ("issue_navigator", "issue-navigator"),
    ("user_preferences", "user-preferences"),
    ("admin_preferences", "admin-preferences"),
    // Dialogs without a Help button that F1 still knows.
    ("New Project", "new-project"),
    (crate::activity_wizard::SALT, "new-activity-wizard"),
    (crate::resource_wizard::SALT, "new-resource-wizard"),
    ("xer_import", "import-wizard"),
    ("export", "export-wizard"),
    ("print_preview", "print-preview"),
    ("report_preview_pages", "print-preview"),
    ("Find", "find-and-replace"),
    ("Replace", "find-and-replace"),
    ("Assign Predecessors", "assign-relationships"),
    ("Assign Successors", "assign-relationships"),
    ("assign_relationship", "assign-relationships"),
    ("assign_record", "assign-dialogs"),
    ("assign_resources", "assign-dialogs"),
    ("add_code_value", "assign-dialogs"),
    ("Assign Resources", "assign-dialogs"),
    ("Assign Roles", "assign-dialogs"),
    ("Assign Resources by Role", "assign-dialogs"),
    ("Assign Activity Codes", "assign-dialogs"),
    ("Assign Notebook Topic", "assign-dialogs"),
    ("Assign Project Codes", "assign-dialogs"),
    ("assign_notebook_topic", "assign-dialogs"),
    ("assign_project_codes", "assign-dialogs"),
    ("Assign Work Products and Documents", "assign-dialogs"),
    ("Assign Risks", "assign-dialogs"),
    ("Add Code Value", "assign-dialogs"),
    ("calendars_used_by", "calendars"),
    ("calendars_in_use", "calendars"),
    ("calendars_replacement", "calendars"),
    ("copy_wbs_options", "view-wbs"),
    ("Delete Calendar", "calendars"),
    ("Check Schedule Results", "check-schedule"),
    ("Schedule Log", "schedule"),
    ("ts_picker", "page-setup"),
    ("grid_save_as", "open-layout"),
    ("pick_group", "report-groups"),
    ("report_wizard", "view-reports"),
    ("Customize", "customize-toolbars"),
    ("External Applications", "external-applications"),
    ("ai_connect", "working-with-ai-agents"),
    ("ai_about", "working-with-ai-agents"),
    ("Connection Status", "working-with-ai-agents"),
    ("chat", "ai-chat"),
    ("chat_options", "ai-chat"),
    ("chat_checks", "ai-chat"),
    ("chat_endpoint", "ai-chat"),
    ("ai_skills", "skills-for-ai-agents"),
    ("ai_skill_edit", "skills-for-ai-agents"),
    ("ai_skill_delete", "skills-for-ai-agents"),
    ("Activity Details", "customize-activity-details"),
];

/// The topic F1 opens for a main view when no dialog is open.
pub(crate) fn view_topic(view: views::MainView) -> &'static str {
    use views::MainView as V;
    match view {
        V::Activities | V::Activities2 => "view-activities",
        V::Wbs => "view-wbs",
        V::Projects => "view-projects",
        V::Resources => "view-resources",
        V::Reports => "view-reports",
        V::Assignments => "view-assignments",
        V::Expenses => "view-expenses",
        V::Documents => "view-documents",
        V::Risks => "view-risks",
        V::Issues => "view-issues",
        V::Thresholds => "view-thresholds",
        V::Report(_) => "view-report-tab",
    }
}

/// The topic a dialog's Help button and F1 open, by the dialog's name.
pub(crate) fn dialog_topic(name: &str) -> Option<&'static str> {
    CONTEXTS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

// ------------------------------------------------------------------ topics

// The Markdown subset is the shared reader in `crate::markdown`.
pub(crate) use crate::markdown::{block_text, Block, Span};

#[derive(Clone, Debug)]
pub(crate) struct Topic {
    pub id: &'static str,
    pub title: String,
    pub blocks: Vec<Block>,
    /// Lower-case plain text of the whole topic, for search.
    pub text: String,
}

/// A section of the table of contents.
#[derive(Clone, Debug)]
pub(crate) struct Section {
    pub title: String,
    pub topics: Vec<String>,
}

pub(crate) struct Library {
    pub topics: Vec<Topic>,
    pub contents: Vec<Section>,
}

impl Library {
    pub fn topic(&self, id: &str) -> Option<&Topic> {
        self.topics.iter().find(|t| t.id == id)
    }

    /// Topics holding every word of `query` (case blind), those with a word
    /// in the title first, then by how often the words occur.
    pub fn search(&self, query: &str) -> Vec<&Topic> {
        let words: Vec<String> = query
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            return vec![];
        }
        let mut hits: Vec<(usize, usize, &Topic)> = self
            .topics
            .iter()
            .filter(|t| words.iter().all(|w| t.text.contains(w.as_str())))
            .map(|t| {
                let title = t.title.to_lowercase();
                let in_title = words.iter().filter(|w| title.contains(w.as_str())).count();
                let count = words
                    .iter()
                    .map(|w| t.text.matches(w.as_str()).count())
                    .sum();
                (in_title, count, t)
            })
            .collect();
        hits.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.cmp(&a.1))
                .then(a.2.title.cmp(&b.2.title))
        });
        hits.into_iter().map(|(_, _, t)| t).collect()
    }
}

/// All topics, parsed once.
pub(crate) fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let mut topics: Vec<Topic> = SOURCES.iter().map(|(id, text)| parse(id, text)).collect();
        topics.push(parse(WHATS_NEW, &whats_new_source(CHANGELOG)));
        Library {
            topics,
            contents: parse_contents(CONTENTS),
        }
    })
}

/// Reads one topic: `# Title` on the first line, then the blocks of the
/// shared Markdown subset (`crate::markdown`), `[text](topic-id)` links
/// included.
pub(crate) fn parse(id: &'static str, source: &str) -> Topic {
    let mut title = String::new();
    let mut body = String::with_capacity(source.len());
    for line in source.lines() {
        match line.trim_end().strip_prefix("# ") {
            Some(t) => title = t.trim().to_owned(),
            None => {
                body.push_str(line);
                body.push('\n');
            }
        }
    }
    let blocks = crate::markdown::blocks(
        &body,
        crate::markdown::Dialect {
            links: true,
            streaming: false,
        },
    );
    let mut text = title.to_lowercase();
    for b in &blocks {
        text.push('\n');
        text.push_str(&block_text(b).to_lowercase());
    }
    Topic {
        id,
        title,
        blocks,
        text,
    }
}

/// The table of contents: `## Section` lines, each followed by `- topic-id`
/// lines.
pub(crate) fn parse_contents(source: &str) -> Vec<Section> {
    let mut out: Vec<Section> = vec![];
    for line in source.lines().map(str::trim) {
        if let Some(t) = line.strip_prefix("## ") {
            out.push(Section {
                title: t.trim().to_owned(),
                topics: vec![],
            });
        } else if let Some(id) = line.strip_prefix("- ") {
            if let Some(s) = out.last_mut() {
                s.topics.push(id.trim().to_owned());
            }
        }
    }
    out
}

// ------------------------------------------------------------ the viewer state

/// The help window: which topic shows, the history and the search text.
#[derive(Default)]
pub(crate) struct HelpState {
    pub open: bool,
    pub topic: String,
    pub back: Vec<String>,
    pub forward: Vec<String>,
    pub query: String,
    /// A Help button that does not know its dialog asked for context help;
    /// the topmost dialog decides next frame.
    pub context_pending: bool,
    /// The topic body scrolls back to its top on the next frame.
    pub scroll_top: bool,
    /// How much the user enlarged the window past its starting size.
    pub grow: Vec2,
}

impl HelpState {
    /// Shows `id`, remembering the topic it leaves for Back.
    pub fn go(&mut self, id: &str) {
        if self.open && !self.topic.is_empty() && self.topic != id {
            self.back.push(std::mem::take(&mut self.topic));
            self.forward.clear();
        }
        self.topic = id.to_owned();
        self.open = true;
        self.scroll_top = true;
    }

    pub fn back(&mut self) {
        if let Some(t) = self.back.pop() {
            self.forward.push(std::mem::replace(&mut self.topic, t));
            self.scroll_top = true;
        }
    }

    pub fn forward(&mut self) {
        if let Some(t) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.topic, t));
            self.scroll_top = true;
        }
    }
}

impl App {
    /// A dialog's Help button: opens the topic for the dialog `name` (its
    /// dialog kit name). An unknown name opens the overview; the help tests
    /// make sure there is none.
    pub(crate) fn open_help_for(&mut self, name: &str) {
        self.help.go(dialog_topic(name).unwrap_or(OVERVIEW));
    }

    /// A Help button shared by several dialogs: the topmost open dialog
    /// decides which topic opens, as with F1.
    pub(crate) fn request_context_help(&mut self) {
        self.help.context_pending = true;
    }

    pub(crate) fn close_help(&mut self) {
        let h = &mut self.help;
        h.open = false;
        h.query.clear();
        h.back.clear();
        h.forward.clear();
    }

    /// Help > What's New... and the toolbar's What's New button.
    pub(crate) fn open_whats_new(&mut self) {
        self.help.go(WHATS_NEW);
    }

    /// Help > Help Topics..., the toolbar's Help button and Ctrl+F1.
    pub(crate) fn open_help_contents(&mut self) {
        self.help.go(OVERVIEW);
    }

    /// The topic F1 opens now: the topmost dialog's, or the current view's.
    pub(crate) fn context_topic(&self, ctx: &egui::Context) -> &'static str {
        let top = crate::dialog_kit::open_dialogs(ctx)
            .into_iter()
            .filter(|n| n != WINDOW_TITLE)
            .find_map(|n| dialog_topic(&n));
        top.unwrap_or_else(|| view_topic(self.views.groups.current()))
    }

    /// F1 and Ctrl+F1, read first in the frame so they work over any dialog.
    pub(crate) fn help_keys(&mut self, ctx: &egui::Context) {
        // Help sits above every dialog, so Escape closes it and nothing else.
        if self.help.open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.close_help();
        }
        // Exact modifiers: Alt+F1 stays Cell Tips (egui's consume_key would
        // take it as F1).
        let take = |ctx: &egui::Context, want: egui::Modifiers| {
            ctx.input_mut(|i| {
                let at = i.events.iter().position(|e| {
                    matches!(e, egui::Event::Key { key: Key::F1, pressed: true, modifiers, .. }
                        if modifiers.alt == want.alt
                            && modifiers.shift == want.shift
                            && (modifiers.ctrl || modifiers.command) == want.command)
                });
                at.map(|at| i.events.remove(at)).is_some()
            })
        };
        if take(ctx, egui::Modifiers::COMMAND) {
            self.open_help_contents();
        } else if take(ctx, egui::Modifiers::NONE) {
            self.help.context_pending = true;
        }
        if std::mem::take(&mut self.help.context_pending) {
            let topic = self.context_topic(ctx);
            self.help.go(topic);
        }
    }
}

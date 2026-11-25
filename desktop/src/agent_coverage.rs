//! Guard: every command the app draws is mapped to how an AI agent does
//! the same through `openxer-cli` and MCP, or marked as a view preference,
//! a session action or a known gap. Drawn commands are collected headless:
//! every menu and submenu of the menu bar, the Activity Details tabs, and
//! the command bars of the views the Project and Enterprise menus open.
//! A command that is drawn but not listed in [`REGISTRY`] fails the test,
//! and so does an entry naming an operation or command `openxer-automation`
//! does not have. `automation/COVERAGE.md` is the readable form of this
//! table and must name every gap listed here.
use crate::qa_sweep::{Driver, MENUS};
use std::collections::BTreeSet;

/// How an agent does what a drawn command does.
#[derive(Clone, Copy, Debug)]
pub enum Agent {
    /// `apply` operations (also single edits and MCP tools).
    Ops(&'static [&'static str]),
    /// MCP tools without their `openxer_` prefix (each is also an
    /// `openxer-cli` command).
    Tools(&'static [&'static str]),
    /// A view preference, layout or navigation; changes no project data.
    View,
    /// An action of the running app (open, save, undo, clipboard, exit);
    /// every automation command reads its input and writes a new output.
    Session,
    /// Not available to agents yet: the start of the COVERAGE.md row that
    /// marks the gap.
    Missing(&'static str),
}
use Agent::*;

/// Drawn command ids (a trailing `*` matches a prefix) and their mapping.
#[rustfmt::skip]
pub const REGISTRY: &[(&str, Agent)] = &[
    // ---------------------------------------------------------------- File
    ("menu_new", Tools(&["eps_edit"])),
    ("menu_open", Session),
    ("menu_close_all", Session),
    ("menu_page_setup", View),
    ("menu_print_preview", View),
    ("menu_print", Tools(&["export_pdf"])),
    ("file_import", Tools(&["import", "import_xlsx"])),
    ("file_export", Tools(&["export", "export_xlsx"])),
    ("save", Session),
    ("menu_refresh", Session),
    ("menu_exit", Session),
    // ---------------------------------------------------------------- Edit
    ("undo", Session),
    ("redo", Session),
    ("edit_copy", Session),
    ("edit_cut", Ops(&["set_activity"])),
    ("edit_paste", Missing("Copy and paste activities")),
    ("menu_add", Ops(&["add_activity"])),
    ("edit_delete", Ops(&["delete_activity"])),
    ("edit_dissolve", Ops(&["dissolve_activity"])),
    ("edit_renumber", Ops(&["set_activity"])),
    ("menu_assign_resources", Ops(&["assign_resource"])),
    ("menu_assign_by_role", Missing("Assign by role, role only")),
    ("menu_assign_roles", Missing("Assign by role, role only")),
    ("menu_assign_codes", Ops(&["set_activity_code"])),
    ("menu_assign_predecessors", Ops(&["relationship"])),
    ("menu_assign_successors", Ops(&["relationship"])),
    ("edit_link", Ops(&["relationship"])),
    ("edit_unlink", Ops(&["relationship"])),
    ("edit_fill_down", Ops(&["set_activity"])),
    ("edit_select_all", View),
    ("menu_find", View),
    ("menu_find_next", View),
    ("menu_replace", Ops(&["set_activity"])),
    ("menu_spell_check", Ops(&["set_activity", "set_notebook", "set_step"])),
    ("menu_user_preferences", Session),
    // ---------------------------------------------------------------- View
    ("layout_*", View),
    ("menu_new_layout", View),
    ("menu_top_*", View),
    ("menu_bottom_*", View),
    ("menu_no_bottom", View),
    ("view_*", View),
    ("menu_columns_set_*", View),
    ("menu_group_set_*", View),
    ("menu_filter*", View),
    ("menu_spotlight", View),
    ("menu_progress_line", View),
    ("menu_line_numbers", View),
    ("menu_view_check_schedule", View),
    ("menu_check_*", View),
    ("menu_acp_*", Missing("Activity critical path through one activity")),
    ("menu_table_font", View),
    ("menu_chart_font", View),
    ("menu_reorganize", View),
    ("menu_collapse_*", View),
    ("menu_expand_all", View),
    ("menu_launch_*", View),
    ("menu_status_bar", View),
    ("menu_zoom_*", View),
    ("zoom_preset_*", View),
    ("menu_toolbar_*", View),
    ("menu_customize_toolbars", View),
    ("menu_lock_toolbars", View),
    ("menu_reset_toolbar*", View),
    ("menu_tab_*", View),
    ("menu_additional_activities", View),
    // ------------------------------------------------------------- Project
    ("menu_activities", View),
    ("menu_resource_assignments", View),
    ("menu_wbs", View),
    ("menu_expenses", View),
    ("menu_documents", View),
    ("menu_thresholds", View),
    ("menu_issues", View),
    ("menu_risks", View),
    ("menu_assign_baselines", Tools(&["baseline_list", "baseline_edit"])),
    ("menu_maintain_baselines", Tools(&["baseline_list", "baseline_edit", "baseline_update"])),
    ("menu_revert_baseline", Tools(&["baseline_edit"])),
    ("menu_set_default_project", Session),
    // ---------------------------------------------------------- Enterprise
    ("menu_projects", View),
    ("menu_eps", Tools(&["eps_list", "eps_edit"])),
    ("menu_resources", Ops(&["add_resource", "set_resource", "delete_resource", "merge_resources", "set_resource_limit"])),
    ("menu_roles", Missing("Roles, OBS, shifts")),
    ("menu_obs", Missing("Roles, OBS, shifts")),
    ("menu_resource_codes", Missing("Project, resource, role and assignment codes")),
    ("menu_project_codes", Missing("Project, resource, role and assignment codes")),
    ("menu_activity_codes", Ops(&["add_code_type", "add_code_value", "set_code_value", "delete_code_value"])),
    ("menu_role_codes", Missing("Project, resource, role and assignment codes")),
    ("menu_assignment_codes", Missing("Project, resource, role and assignment codes")),
    ("menu_udf", Missing("User defined fields dictionary")),
    ("menu_calendars", Ops(&["calendar_day", "calendar_weekday", "add_calendar", "copy_calendar", "rename_calendar", "set_default_calendar", "convert_calendar", "delete_calendar", "set_calendar"])),
    ("menu_resource_shifts", Missing("Roles, OBS, shifts")),
    ("menu_step_templates", Missing("Roles, OBS, shifts, funding sources, cost accounts, step templates")),
    ("menu_cost_accounts", Missing("Roles, OBS, shifts, funding sources, cost accounts")),
    ("menu_funding_sources", Missing("Roles, OBS, shifts, funding sources")),
    ("menu_resource_curves", Tools(&["curves"])),
    ("menu_external_apps", View),
    // --------------------------------------------------------------- Tools
    ("menu_schedule", Ops(&["schedule", "set_schedule_option"])),
    ("menu_check_schedule", Tools(&["check_schedule"])),
    ("menu_level_resources", Ops(&["schedule", "set_schedule_option"])),
    ("menu_apply_actuals", Ops(&["apply_actuals"])),
    ("menu_update_progress", Ops(&["update_progress"])),
    ("menu_recalc_costs", Ops(&["recalculate_costs"])),
    ("menu_store_period", Missing("Store Period Performance")),
    ("tools_global_change", Ops(&["global_change"])),
    ("menu_monitor_thresholds", Ops(&["monitor_thresholds"])),
    ("menu_issue_navigator", View),
    ("menu_reports", Missing("Reports, report wizard")),
    ("menu_report_groups", Missing("Reports, report wizard")),
    ("menu_batch_reports", Missing("Reports, report wizard")),
    ("menu_report_wizard", Missing("Reports, report wizard")),
    ("menu_scurve", Tools(&["export_scurve"])),
    ("menu_earned_hours", Tools(&["export_earned_hours"])),
    ("menu_project_checker", Missing("Project Checker")),
    ("menu_top_down", Ops(&["top_down_estimate"])),
    ("menu_schedule_comparison", Missing("Schedule Comparison")),
    // ------------------------------------------------- AI agents, admin, help
    ("menu_ai_*", Session),
    ("menu_admin_preferences", View),
    ("menu_share_dictionaries", Tools(&["workspace_migrate"])),
    ("menu_admin_categories", Missing("Admin categories, currencies")),
    ("menu_currencies", Missing("Admin categories, currencies")),
    ("menu_fin_calendars", Missing("Admin categories, currencies, financial period calendars")),
    ("menu_about", View),
    ("menu_local_help", View),
    ("menu_context_help", View),
    ("menu_hint_help", View),
    ("menu_whats_new", View),
    // ------------------------------------------------ Activity Details tabs
    ("detail_tab_general", Ops(&["set_activity"])),
    ("detail_tab_status", Ops(&["set_activity"])),
    ("detail_tab_resources", Ops(&["assign_resource", "remove_assignment", "set_assignment"])),
    ("detail_tab_relationships", Ops(&["relationship"])),
    ("detail_tab_predecessors", Ops(&["relationship"])),
    ("detail_tab_successors", Ops(&["relationship"])),
    ("detail_tab_codes", Ops(&["set_activity_code"])),
    ("detail_tab_notebook", Ops(&["set_notebook"])),
    ("detail_tab_steps", Ops(&["add_step", "set_step", "delete_step", "move_step", "add_steps_from_template", "create_step_template"])),
    ("detail_tab_expenses", Ops(&["add_expense", "set_expense", "delete_expense"])),
    ("detail_tab_documents", Ops(&["link_document", "unlink_document", "set_document"])),
    ("detail_tab_feedback", Ops(&["set_feedback"])),
    ("detail_tab_risks", Ops(&["assign_risk", "unassign_risk"])),
    ("detail_tab_udf", Ops(&["set_udf"])),
    ("detail_tab_summary", Tools(&["load", "earned_value"])),
    // --------------------------------------------------- view command bars
    ("wbs_cmd_Add", Ops(&["add_wbs"])),
    ("wbs_cmd_Shift *", Ops(&["shift_wbs"])),
    ("wbs_cmd_Cut", Missing("Cut, copy, paste, delete or merge WBS elements")),
    ("wbs_cmd_Copy", Missing("Cut, copy, paste, delete or merge WBS elements")),
    ("wbs_cmd_Paste", Missing("Cut, copy, paste, delete or merge WBS elements")),
    ("wbs_cmd_Del. / Merge", Missing("Cut, copy, paste, delete or merge WBS elements")),
    ("expenses_cmd_add", Ops(&["add_expense"])),
    ("expenses_cmd_delete", Ops(&["delete_expense"])),
    ("documents_cmd_add", Ops(&["add_document"])),
    ("documents_cmd_delete", Ops(&["delete_document"])),
    ("issues_cmd_add", Ops(&["add_issue"])),
    ("issues_cmd_delete", Ops(&["delete_issue"])),
    ("risks_cmd_add", Ops(&["add_risk"])),
    ("risks_cmd_delete", Ops(&["delete_risk"])),
    ("thresholds_cmd_add", Ops(&["add_threshold"])),
    ("thresholds_cmd_delete", Ops(&["delete_threshold"])),
    ("thresholds_cmd_monitor", Ops(&["monitor_thresholds"])),
    ("projects_cmd_add", Tools(&["eps_edit"])),
    ("projects_cmd_cut", Tools(&["eps_edit"])),
    ("projects_cmd_paste", Tools(&["eps_edit"])),
    ("projects_cmd_copy", Missing("Copy or delete a project")),
    ("projects_cmd_delete", Missing("Copy or delete a project")),
    ("projects_cmd_open", Session),
    ("projects_cmd_close", Session),
];

fn lookup(key: &str) -> Option<Agent> {
    REGISTRY.iter().find_map(|(k, a)| {
        let hit = match k.strip_suffix('*') {
            Some(prefix) => key.starts_with(prefix),
            None => *k == key,
        };
        hit.then_some(*a)
    })
}

/// Every command id the app draws in the places the guard visits.
fn drawn() -> BTreeSet<String> {
    let mut d = Driver::new("agent-coverage", openxer_xer::starter());
    let mut out = BTreeSet::new();
    d.home();
    let baseline = d.keys();
    for (menu, subs) in MENUS {
        let title = format!("menu_bar_{menu}");
        let mut groups: Vec<Option<&str>> = vec![None];
        groups.extend(subs.iter().map(|s| Some(*s)));
        for sub in groups {
            d.home();
            d.open_menu(&title);
            if let Some(label) = sub {
                assert!(d.hover_text(label), "{menu} > {label} is drawn");
            }
            out.extend(
                d.keys()
                    .into_iter()
                    .filter(|k| !baseline.contains(k) && !k.starts_with("menu_bar_")),
            );
            d.home();
        }
    }
    d.home();
    out.extend(
        d.keys()
            .into_iter()
            .filter(|k| k.starts_with("detail_tab_")),
    );
    for (menu, item) in [
        ("project", "menu_wbs"),
        ("project", "menu_expenses"),
        ("project", "menu_documents"),
        ("project", "menu_thresholds"),
        ("project", "menu_issues"),
        ("project", "menu_risks"),
        ("project", "menu_resource_assignments"),
        ("enterprise", "menu_projects"),
        ("enterprise", "menu_resources"),
    ] {
        d.home();
        d.open_menu(&format!("menu_bar_{menu}"));
        d.click(item);
        d.frames(2);
        out.extend(d.keys().into_iter().filter(|k| k.contains("_cmd_")));
        out.extend(dock_commands(&d.app));
    }
    d.home();
    out
}

/// The right dock's Edit and Move bars run a view's own commands; the
/// view's command bar draws those only while the bars are hidden. They
/// count as drawn under the names the view's bar gives them.
fn dock_commands(app: &crate::App) -> Vec<String> {
    use crate::views::{MainView, ViewEdit};
    let edits = [
        (ViewEdit::Add, "add", "Add"),
        (ViewEdit::Delete, "delete", "Del. / Merge"),
        (ViewEdit::Cut, "cut", "Cut"),
        (ViewEdit::Copy, "copy", "Copy"),
        (ViewEdit::Paste, "paste", "Paste"),
    ];
    let view = match app.views.main {
        MainView::Wbs => "wbs",
        MainView::Projects => "projects",
        MainView::Expenses => "expenses",
        MainView::Documents => "documents",
        MainView::Issues => "issues",
        MainView::Risks => "risks",
        MainView::Thresholds => "thresholds",
        _ => return vec![],
    };
    let mut out: Vec<String> = edits
        .iter()
        .filter(|(edit, _, _)| app.view_edit_enabled(*edit).is_some())
        .map(|(_, plain, wbs)| {
            let name = if view == "wbs" { wbs } else { plain };
            format!("{view}_cmd_{name}")
        })
        .collect();
    if view == "wbs" {
        out.push("wbs_cmd_Shift up".into());
    }
    out
}

fn coverage_md() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../automation/COVERAGE.md");
    std::fs::read_to_string(path).expect("automation/COVERAGE.md")
}

/// The registry names only operations and tools the automation layer has,
/// and every gap it lists is a gap row of COVERAGE.md.
#[test]
fn every_registry_entry_names_real_operations_tools_and_gaps() {
    let tools: BTreeSet<String> = openxer_automation::mcp::tool_definitions()
        .into_iter()
        .map(|t| {
            t["name"]
                .as_str()
                .unwrap()
                .trim_start_matches("openxer_")
                .to_string()
        })
        .collect();
    let ops: BTreeSet<&str> = openxer_automation::edit::OPERATIONS
        .iter()
        .copied()
        .collect();
    let coverage = coverage_md();
    let mut wrong = vec![];
    let mut seen = BTreeSet::new();
    for (key, agent) in REGISTRY {
        if !seen.insert(*key) {
            wrong.push(format!("{key} is listed twice"));
        }
        match agent {
            Ops(list) => wrong.extend(
                list.iter()
                    .filter(|o| !ops.contains(*o))
                    .map(|o| format!("{key}: no operation {o}")),
            ),
            Tools(list) => wrong.extend(
                list.iter()
                    .filter(|t| !tools.contains(**t))
                    .map(|t| format!("{key}: no MCP tool openxer_{t}")),
            ),
            Missing(row) => {
                let gap = coverage
                    .lines()
                    .any(|l| l.starts_with(&format!("| {row}")) && l.contains("**gap**"));
                if !gap {
                    wrong.push(format!(
                        "{key}: COVERAGE.md has no gap row starting {row:?}"
                    ));
                }
            }
            View | Session => {}
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Every command the app draws is in the registry, and every exact entry is
/// still drawn (Activity Details tabs can be hidden, so they may be absent).
#[test]
fn every_drawn_command_maps_to_an_agent_operation_or_is_marked() {
    let drawn = drawn();
    let unmapped: Vec<&String> = drawn.iter().filter(|k| lookup(k).is_none()).collect();
    assert!(
        unmapped.is_empty(),
        "commands the app draws with no agent mapping; add each to REGISTRY in          desktop/src/agent_coverage.rs (an apply operation, an MCP tool, View, Session,          or Missing with a gap row in automation/COVERAGE.md): {unmapped:#?}"
    );
    let stale: Vec<&str> = REGISTRY
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| !k.ends_with('*') && !k.starts_with("detail_tab_") && !drawn.contains(*k))
        .collect();
    assert!(
        stale.is_empty(),
        "registry entries the app no longer draws: {stale:?}"
    );
}

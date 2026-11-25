//! Executes live desktop UI commands queued by the `openxer_ui` chat tool
//! (filters, grouping/sorting, columns, selection, critical paths, view/pane
//! navigation, Gantt zoom, dialogs, exports, undo/redo) directly on [`App`].
use crate::critical_path::Direction;
use crate::details::DetailTab;
use crate::grid::columns::{self, Column};
use crate::grid::filters::{Criterion, FilterSelection, Op, Standard, UserFilter};
use crate::grid::layout::{ColumnSetting, GroupLevel, SortKey};
use crate::layout_opts::checks::CheckFilter;
use crate::panes::{BottomView, TopView};
use crate::views::MainView;
use crate::App;
use eframe::egui;
use serde_json::Value;

const CHAT_FILTER_NAME: &str = "Chat Filter";

fn resolve_column<'a>(catalog: &'a [Column], name: &str) -> Option<&'a Column> {
    let want = name.trim();
    if want.is_empty() {
        return None;
    }
    let lower = want.to_ascii_lowercase();
    let norm = lower.replace([' ', '-', '/'], "_");
    // Common aliases LLMs use from openxer_load fields.
    let alias = match norm.as_str() {
        "task_code" | "activity_id" | "id" => Some("activity_id"),
        "name" | "task_name" | "activity_name" => Some("activity_name"),
        "status" | "status_code" | "activity_status" => Some("status"),
        "total_float" | "total_float_minutes" | "tf" => Some("total_float"),
        "free_float" | "free_float_minutes" | "ff" => Some("free_float"),
        "original_duration" | "original_duration_minutes" | "od" => Some("original_duration"),
        "remaining_duration" | "remaining_duration_minutes" | "rd" => Some("remaining_duration"),
        "early_start" | "es" => Some("early_start"),
        "early_finish" | "ef" => Some("early_finish"),
        "late_start" | "ls" => Some("late_start"),
        "late_finish" | "lf" => Some("late_finish"),
        "actual_start" | "as" => Some("actual_start"),
        "actual_finish" | "af" => Some("actual_finish"),
        "wbs" | "wbs_code" => Some("wbs"),
        "wbs_name" => Some("wbs_name"),
        "primary_resource" | "resource" => Some("primary_resource"),
        "calendar" | "calendar_id" => Some("calendar"),
        "constraint" | "primary_constraint" => Some("primary_constraint"),
        "constraint_date" | "primary_constraint_date" => Some("primary_constraint_date"),
        "critical" => Some("critical"),
        "longest_path" | "driving_path" => Some("longest_path"),
        _ => None,
    };
    if let Some(id) = alias {
        if let Some(c) = catalog.iter().find(|c| c.id.eq_ignore_ascii_case(id)) {
            return Some(c);
        }
    }
    catalog
        .iter()
        .find(|c| {
            c.id.eq_ignore_ascii_case(want)
                || c.id.eq_ignore_ascii_case(&norm)
                || c.title.eq_ignore_ascii_case(want)
        })
        .or_else(|| {
            catalog
                .iter()
                .find(|c| c.title.to_ascii_lowercase().contains(&lower))
        })
}

fn parse_op(text: &str) -> Op {
    match text
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
        .as_str()
    {
        "equals" | "equal" | "eq" | "==" | "=" | "is" => Op::Equals,
        "not_equals" | "not_equal" | "ne" | "!=" | "is_not" => Op::NotEqual,
        "contains" | "includes" => Op::Contains,
        "not_contains" | "does_not_contain" => Op::NotContains,
        "starts_with" | "begins_with" => Op::StartsWith,
        "less" | "less_than" | "lt" | "<" | "before" => Op::Less,
        "less_or_equal" | "less_than_or_equal" | "lte" | "<=" => Op::LessOrEqual,
        "greater" | "greater_than" | "gt" | ">" | "after" => Op::Greater,
        "greater_or_equal" | "greater_than_or_equal" | "gte" | ">=" => Op::GreaterOrEqual,
        "within" | "between" | "within_range" => Op::Within,
        "not_within" | "not_between" => Op::NotWithin,
        "is_under" | "under" => Op::IsUnder,
        _ => Op::Equals,
    }
}

fn resolve_named_filter(name: &str, users: &[UserFilter]) -> Option<(String, String)> {
    let want = name.trim();
    if want.is_empty() {
        return None;
    }
    let lower = want.to_ascii_lowercase();
    let norm = lower.replace(['/', '-', ' '], "_");
    for s in Standard::ALL {
        if s.caption().eq_ignore_ascii_case(want)
            || s.key().eq_ignore_ascii_case(want)
            || s.key().trim_start_matches("std:") == norm
        {
            return Some((s.key(), s.caption().to_string()));
        }
    }
    for c in CheckFilter::ALL {
        if c.caption().eq_ignore_ascii_case(want)
            || c.key().eq_ignore_ascii_case(want)
            || c.key().trim_start_matches("check:") == norm
            || c.caption().to_ascii_lowercase().contains(&lower)
        {
            return Some((c.key(), c.caption().to_string()));
        }
    }
    for u in users {
        if u.name.eq_ignore_ascii_case(want) || u.key().eq_ignore_ascii_case(want) {
            return Some((u.key(), u.name.clone()));
        }
    }
    None
}

impl App {
    /// Execute one queued `openxer_ui` command payload on the live app window.
    /// Returns a short user-facing summary line for the chat transcript/status.
    pub(crate) fn apply_chat_ui_action(
        &mut self,
        ctx: &egui::Context,
        payload: &Value,
    ) -> Result<String, String> {
        let action = payload["action"]
            .as_str()
            .ok_or_else(|| "UI action missing action field".to_string())?;
        let summary = match action {
            "filter" => self.ui_action_filter(payload)?,
            "clear_filter" => self.ui_action_clear_filter(),
            "group_sort" => self.ui_action_group_sort(payload)?,
            "columns" => self.ui_action_columns(payload)?,
            "select" => self.ui_action_select(payload)?,
            "critical_path" => self.ui_action_critical_path(payload)?,
            "view" => self.ui_action_view(payload)?,
            "gantt" => self.ui_action_gantt(payload)?,
            "open_dialog" => self.ui_action_open_dialog(payload)?,
            "export" => self.ui_action_export(payload)?,
            "undo" => {
                self.history_move(ctx, false);
                "Undid the last schedule edit.".into()
            }
            "redo" => {
                self.history_move(ctx, true);
                "Redid the last schedule edit.".into()
            }
            other => return Err(format!("Unknown UI action {other:?}")),
        };
        self.status = summary.clone();
        Ok(summary)
    }

    fn ensure_activities_view(&mut self) {
        if !self.views.main.is_activities() {
            self.show_view(MainView::Activities);
        }
    }

    fn ui_action_clear_filter(&mut self) -> String {
        self.hide_critical_paths();
        self.grid.layout.filter = FilterSelection::default();
        self.drop_frame_cache();
        "Cleared filter (showing All Activities).".into()
    }

    fn ui_action_filter(&mut self, payload: &Value) -> Result<String, String> {
        self.ensure_activities_view();
        self.hide_critical_paths();
        let highlight = payload["highlight"].as_bool().unwrap_or(false);
        let match_all = payload["match_all"].as_bool().unwrap_or(true);

        // 1. Explicit list of Activity IDs.
        if let Some(arr) = payload["activities"].as_array().filter(|a| !a.is_empty()) {
            let ids: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
                .map(str::to_string)
                .collect();
            if ids.is_empty() {
                return Err("Filter activities list was empty.".into());
            }
            let name = payload["save_as"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(CHAT_FILTER_NAME)
                .to_string();
            let filter = UserFilter {
                name: name.clone(),
                match_all: false,
                rows: ids
                    .iter()
                    .map(|id| Criterion {
                        column: "activity_id".into(),
                        op: Op::Equals,
                        value: id.clone(),
                        high: String::new(),
                        level: 0,
                    })
                    .collect(),
            };
            self.install_user_filter(filter, highlight);
            return Ok(format!(
                "Filtered to {} activit{} ({name}).",
                ids.len(),
                if ids.len() == 1 { "y" } else { "ies" }
            ));
        }

        // 2. Ad-hoc criteria rows (accept "criteria" or "rules").
        if let Some(arr) = payload["criteria"]
            .as_array()
            .or_else(|| payload["rules"].as_array())
            .filter(|a| !a.is_empty())
        {
            let catalog = columns::catalog(&self.project);
            let mut rows = vec![];
            for item in arr {
                let col_name = item["column"]
                    .as_str()
                    .or_else(|| item["field"].as_str())
                    .unwrap_or("");
                let col = resolve_column(&catalog, col_name)
                    .ok_or_else(|| format!("Unknown filter column {col_name:?}"))?;
                let op = parse_op(item["op"].as_str().unwrap_or("equals"));
                let value = item["value"].as_str().unwrap_or("").to_string();
                let high = item["high"].as_str().unwrap_or("").to_string();
                rows.push(Criterion {
                    column: col.id.clone(),
                    op,
                    value,
                    high,
                    level: 0,
                });
            }
            let name = payload["save_as"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(CHAT_FILTER_NAME)
                .to_string();
            let count = rows.len();
            let filter = UserFilter {
                name: name.clone(),
                match_all,
                rows,
            };
            self.install_user_filter(filter, highlight);
            return Ok(format!(
                "Applied custom filter \"{name}\" ({count} rule{}).",
                if count == 1 { "" } else { "s" }
            ));
        }

        // 3. Named standard, Check Schedule, or user-defined filter.
        if let Some(name) = payload["filter"].as_str() {
            if name.trim().eq_ignore_ascii_case("all")
                || name.trim().eq_ignore_ascii_case("all activities")
                || name.trim().eq_ignore_ascii_case("none")
            {
                return Ok(self.ui_action_clear_filter());
            }
            let Some((key, label)) = resolve_named_filter(name, &self.grid.user_filters) else {
                return Err(format!("Unknown filter {name:?}."));
            };
            self.grid.layout.filter = FilterSelection {
                all_activities: false,
                selected: vec![key],
                match_all: true,
                highlight,
            };
            self.drop_frame_cache();
            return Ok(format!(
                "{} filter: {label}.",
                if highlight { "Highlighted" } else { "Applied" }
            ));
        }

        Err("filter needs filter, activities, or criteria.".into())
    }

    fn install_user_filter(&mut self, filter: UserFilter, highlight: bool) {
        let key = filter.key();
        self.grid.user_filters.retain(|u| u.name != filter.name);
        self.grid.user_filters.push(filter);
        self.grid.layout.filter = FilterSelection {
            all_activities: false,
            selected: vec![key],
            match_all: true,
            highlight,
        };
        self.drop_frame_cache();
    }

    fn ui_action_group_sort(&mut self, payload: &Value) -> Result<String, String> {
        self.ensure_activities_view();
        let catalog = columns::catalog(&self.project);
        let mut notes = vec![];
        if let Some(groups) = payload["group_by"].as_array() {
            let to_level = payload["to_level"].as_u64().map(|n| n as usize);
            let mut levels = vec![];
            let mut names = vec![];
            for g in groups {
                let Some(name) = g.as_str() else {
                    continue;
                };
                if name.trim().eq_ignore_ascii_case("none") {
                    continue;
                }
                let col = resolve_column(&catalog, name)
                    .ok_or_else(|| format!("Unknown group column {name:?}"))?;
                names.push(col.title.clone());
                levels.push(GroupLevel {
                    column: col.id.clone(),
                    to_level: if col.id == "wbs" { to_level } else { None },
                    indent: col.id == "wbs",
                    interval: String::new(),
                });
            }
            self.grid.layout.group_sort.levels = levels;
            if names.is_empty() {
                notes.push("Ungrouped".into());
            } else {
                notes.push(format!("Grouped by {}", names.join(", ")));
            }
        }
        if let Some(sorts) = payload["sort_by"].as_array() {
            let mut keys = vec![];
            let mut names = vec![];
            for s in sorts {
                let col_name = s["column"].as_str().unwrap_or("");
                let col = resolve_column(&catalog, col_name)
                    .ok_or_else(|| format!("Unknown sort column {col_name:?}"))?;
                let desc = s["descending"].as_bool().unwrap_or(false);
                names.push(format!(
                    "{}{}",
                    col.title,
                    if desc { " (desc)" } else { "" }
                ));
                keys.push(SortKey {
                    column: col.id.clone(),
                    descending: desc,
                });
            }
            self.grid.layout.group_sort.sort = keys;
            notes.push(format!("Sorted by {}", names.join(", ")));
        }
        if let Some(v) = payload["show_group_totals"].as_bool() {
            self.grid.layout.group_sort.show_group_totals = v;
        }
        if let Some(v) = payload["show_summaries_only"].as_bool() {
            self.grid.layout.group_sort.show_summaries_only = v;
        }
        if let Some(v) = payload["hide_if_empty"].as_bool() {
            self.grid.layout.group_sort.hide_if_empty = v;
        }
        self.drop_frame_cache();
        if payload["expand_all"].as_bool() == Some(true) {
            self.expand_all_bands();
            notes.push("Expanded all bands".into());
        } else if payload["collapse_all"].as_bool() == Some(true) {
            self.collapse_all_bands();
            notes.push("Collapsed all bands".into());
        }
        if notes.is_empty() {
            notes.push("Updated Group and Sort".into());
        }
        Ok(format!("{}.", notes.join("; ")))
    }

    fn ui_action_columns(&mut self, payload: &Value) -> Result<String, String> {
        self.ensure_activities_view();
        if payload["reset"].as_bool() == Some(true) {
            self.grid.layout.columns = crate::grid::layout::Layout::default().columns;
            self.drop_frame_cache();
            return Ok("Reset table columns to defaults.".into());
        }
        let catalog = columns::catalog(&self.project);
        let mut notes = vec![];
        if let Some(set) = payload["set"].as_array() {
            let mut next = vec![];
            for item in set {
                let name = item.as_str().unwrap_or("");
                let col = resolve_column(&catalog, name)
                    .ok_or_else(|| format!("Unknown column {name:?}"))?;
                next.push(ColumnSetting::new(&col.id, col.width));
            }
            if next.is_empty() {
                return Err("Cannot set an empty column list.".into());
            }
            let count = next.len();
            self.grid.layout.columns = next;
            notes.push(format!("Set {count} columns"));
        }
        if let Some(add) = payload["add"].as_array() {
            let mut added = vec![];
            for item in add {
                let name = item.as_str().unwrap_or("");
                let col = resolve_column(&catalog, name)
                    .ok_or_else(|| format!("Unknown column {name:?}"))?;
                if !self.grid.layout.columns.iter().any(|c| c.id == col.id) {
                    self.grid
                        .layout
                        .columns
                        .push(ColumnSetting::new(&col.id, col.width));
                    added.push(col.title.clone());
                }
            }
            if !added.is_empty() {
                notes.push(format!("Added columns: {}", added.join(", ")));
            }
        }
        if let Some(remove) = payload["remove"].as_array() {
            let mut removed = vec![];
            for item in remove {
                let name = item.as_str().unwrap_or("");
                if let Some(col) = resolve_column(&catalog, name) {
                    let before = self.grid.layout.columns.len();
                    if before > 1 {
                        self.grid.layout.columns.retain(|c| c.id != col.id);
                        if self.grid.layout.columns.len() < before {
                            removed.push(col.title.clone());
                        }
                    }
                }
            }
            if !removed.is_empty() {
                notes.push(format!("Removed columns: {}", removed.join(", ")));
            }
        }
        self.drop_frame_cache();
        if notes.is_empty() {
            Ok("Columns unchanged.".into())
        } else {
            Ok(format!("{}.", notes.join("; ")))
        }
    }

    fn ui_action_select(&mut self, payload: &Value) -> Result<String, String> {
        let arr = payload["activities"]
            .as_array()
            .ok_or_else(|| "select needs an activities array.".to_string())?;
        let codes: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
            .map(str::to_string)
            .collect();
        if codes.is_empty() {
            return Err("No Activity IDs provided to select.".into());
        }
        self.chat_select_many(&codes);
        Ok(self.status.clone())
    }

    fn ui_action_critical_path(&mut self, payload: &Value) -> Result<String, String> {
        self.ensure_activities_view();
        let dir = payload["direction"].as_str().unwrap_or("both");
        if dir.eq_ignore_ascii_case("off") {
            self.hide_critical_paths();
            return Ok("Turned off Activity Critical Paths.".into());
        }
        if let Some(code) = payload["activity"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
        {
            self.chat_go_to(code.trim());
        }
        if let Some(paths) = payload["paths"].as_u64() {
            self.critical_path.count = (paths as usize).clamp(1, crate::critical_path::MAX_PATHS);
        }
        if let Some(group) = payload["group"].as_bool() {
            self.critical_path.group = group;
        }
        let direction = match dir.to_ascii_lowercase().as_str() {
            "backward" => Direction::Backward,
            "forward" => Direction::Forward,
            _ => Direction::Both,
        };
        // Clear first so calling toggle sets the exact requested direction cleanly.
        self.hide_critical_paths();
        self.toggle_critical_paths(direction);
        Ok(format!(
            "Showing Activity Critical Paths ({dir}, {} paths).",
            self.critical_path.count
        ))
    }

    fn ui_action_view(&mut self, payload: &Value) -> Result<String, String> {
        let mut notes = vec![];
        if let Some(main) = payload["main"].as_str() {
            let v = match main
                .trim()
                .to_ascii_lowercase()
                .replace([' ', '-'], "_")
                .as_str()
            {
                "activities" => MainView::Activities,
                "wbs" => MainView::Wbs,
                "projects" => MainView::Projects,
                "resources" => MainView::Resources,
                "assignments" | "resource_assignments" => MainView::Assignments,
                "expenses" => MainView::Expenses,
                "documents" | "wps_docs" => MainView::Documents,
                "risks" => MainView::Risks,
                "issues" => MainView::Issues,
                "thresholds" => MainView::Thresholds,
                "reports" => MainView::Reports,
                other => return Err(format!("Unknown main view {other:?}")),
            };
            self.show_view(v);
            notes.push(format!("Switched to {main}"));
        }
        if let Some(top) = payload["top"].as_str() {
            self.ensure_activities_view();
            let t = match top
                .trim()
                .to_ascii_lowercase()
                .replace([' ', '-'], "_")
                .as_str()
            {
                "gantt" | "gantt_chart" => TopView::Gantt,
                "table" | "activity_table" => TopView::Table,
                "network" | "activity_network" => TopView::Network,
                "activity_usage_spreadsheet" | "activity_usage" => TopView::ActivityUsageSheet,
                "resource_usage_spreadsheet" | "resource_usage" => TopView::ResourceUsageSheet,
                other => return Err(format!("Unknown top view {other:?}")),
            };
            self.set_top(t);
            notes.push(format!("Top pane: {top}"));
        }
        if let Some(bottom) = payload["bottom"].as_str() {
            self.ensure_activities_view();
            let b = match bottom
                .trim()
                .to_ascii_lowercase()
                .replace([' ', '-'], "_")
                .as_str()
            {
                "none" | "off" | "hide" => None,
                "details" | "activity_details" => Some(BottomView::Details),
                "activity_table" | "table" => Some(BottomView::ActivityTable),
                "gantt" | "gantt_chart" => Some(BottomView::Gantt),
                "activity_usage_spreadsheet" => Some(BottomView::ActivityUsageSheet),
                "resource_usage_spreadsheet" => Some(BottomView::ResourceUsageSheet),
                "activity_usage_profile" => Some(BottomView::ActivityUsageProfile),
                "resource_usage_profile" => Some(BottomView::ResourceUsageProfile),
                "trace_logic" | "trace" => Some(BottomView::TraceLogic),
                other => return Err(format!("Unknown bottom view {other:?}")),
            };
            self.set_bottom(b);
            notes.push(format!("Bottom pane: {bottom}"));
        }
        if let Some(tab) = payload["detail_tab"].as_str() {
            self.ensure_activities_view();
            self.set_bottom(Some(BottomView::Details));
            let dt = match tab
                .trim()
                .to_ascii_lowercase()
                .replace([' ', '-'], "_")
                .as_str()
            {
                "general" => DetailTab::General,
                "status" => DetailTab::Status,
                "predecessors" => DetailTab::Predecessors,
                "successors" => DetailTab::Successors,
                "relationships" => DetailTab::Relationships,
                "resources" => DetailTab::Resources,
                "steps" => DetailTab::Steps,
                "notebook" | "notes" => DetailTab::Notebook,
                "codes" | "activity_codes" => DetailTab::Codes,
                "expenses" => DetailTab::Expenses,
                "documents" | "wps_docs" => DetailTab::Documents,
                "summary" => DetailTab::Summary,
                "feedback" => DetailTab::Feedback,
                "risks" => DetailTab::Risks,
                "udfs" | "user_defined_fields" => DetailTab::UserDefinedFields,
                other => return Err(format!("Unknown details tab {other:?}")),
            };
            self.show_detail_tab(dt);
            notes.push(format!("Details tab: {}", dt.caption()));
        }
        if notes.is_empty() {
            Err("view needs main, top, bottom, or detail_tab.".into())
        } else {
            Ok(format!("{}.", notes.join("; ")))
        }
    }

    fn ui_action_gantt(&mut self, payload: &Value) -> Result<String, String> {
        self.ensure_activities_view();
        let mut notes = vec![];
        if let Some(zoom) = payload["zoom"].as_str() {
            match zoom.to_ascii_lowercase().as_str() {
                "fit" => {
                    self.zoom_to_best_fit();
                    notes.push("Zoomed Gantt to fit".into());
                }
                "in" => {
                    self.zoom_step(true);
                    notes.push("Zoomed in".into());
                }
                "out" => {
                    self.zoom_step(false);
                    notes.push("Zoomed out".into());
                }
                other => return Err(format!("Unknown zoom {other:?}")),
            }
        }
        if let Some(rels) = payload["relationships"].as_bool() {
            self.gantt.options.show_relationships = rels;
            notes.push(if rels {
                "Showed Gantt relationship lines".into()
            } else {
                "Hid Gantt relationship lines".into()
            });
        }
        if let Some(target) = payload["scroll_to"].as_str() {
            if target.eq_ignore_ascii_case("data_date") {
                if let Some(dd) = self.project.model.project.data_date {
                    self.timeline_origin = dd - 7 * crate::DAY;
                    notes.push("Scrolled Gantt to Data Date".into());
                }
            } else {
                self.chat_go_to(target.trim());
                notes.push(format!("Jumped to {target}"));
            }
        }
        if notes.is_empty() {
            Err("gantt needs zoom, relationships, or scroll_to.".into())
        } else {
            Ok(format!("{}.", notes.join("; ")))
        }
    }

    fn ui_action_open_dialog(&mut self, payload: &Value) -> Result<String, String> {
        let dialog = payload["dialog"]
            .as_str()
            .ok_or_else(|| "open_dialog needs a dialog name.".to_string())?;
        match dialog
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "check_schedule" => self.open_check_schedule(),
            "schedule" => self.open_schedule_dialog(),
            "level" | "level_resources" => self.open_level_resources(),
            "update_progress" => self.open_update_progress(),
            "filters" => self.open_filters_dialog(),
            "group_sort" | "group_and_sort" => self.open_group_sort_dialog(),
            "columns" => self.open_columns_dialog(),
            "bars" => self.open_bars_dialog(),
            "timescale" => self.open_timescale_dialog(),
            "calendars" => self.open_calendars(),
            "baselines" | "maintain_baselines" => self.open_maintain_baselines(),
            "assign_baselines" => self.open_assign_baselines(),
            "global_change" => self.open_global_change(),
            "compare" | "schedule_comparison" => self.open_schedule_comparison(),
            "print_preview" | "print" => self.show_print_preview(),
            "page_setup" => self.open_page_setup(),
            "export" => self.open_export_wizard(),
            "import" => self.open_import_wizard(),
            "user_preferences" | "preferences" => self.open_user_preferences(),
            "layouts" => self.open_layout_dialog(),
            "find" => self.open_find(false),
            "issue_navigator" => self.open_issue_navigator(),
            "monitor_thresholds" => self.open_monitor_thresholds(),
            other => return Err(format!("Unknown dialog {other:?}")),
        }
        Ok(format!("Opened {dialog}."))
    }

    fn ui_action_export(&mut self, payload: &Value) -> Result<String, String> {
        let fmt = payload["format"].as_str().unwrap_or("xlsx");
        let path_str = payload["path"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(dest) = path_str else {
            if fmt.eq_ignore_ascii_case("pdf") {
                self.show_print_preview();
                return Ok("Opened Print Preview for PDF export.".into());
            }
            self.open_export_wizard();
            return Ok(format!("Opened Export wizard ({fmt})."));
        };
        let out_path = std::path::PathBuf::from(dest);
        if out_path.exists() {
            return Err(format!("Refused to overwrite existing file {dest}."));
        }
        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Snapshot the open project to a temp XER and run the shared automation exporter.
        let tmp_dir =
            std::env::temp_dir().join(format!("openxer-chat-export-{}", std::process::id()));
        let snap = super::live::work::write_snapshot(&tmp_dir, &self.project)?;
        let req = match fmt.to_ascii_lowercase().as_str() {
            "xlsx" => {
                serde_json::json!({"command": "export_xlsx", "input": snap, "output": out_path})
            }
            "scurve" => {
                serde_json::json!({"command": "export_scurve", "input": snap, "output": out_path})
            }
            "earned_hours" => {
                serde_json::json!({"command": "export_earned_hours", "input": snap, "output": out_path})
            }
            "pdf" => {
                serde_json::json!({"command": "export_pdf", "input": snap, "output": out_path})
            }
            "xer" => serde_json::json!({"command": "export", "input": snap, "output": out_path}),
            "xml" => {
                serde_json::json!({"command": "export", "input": snap, "output": out_path, "format": "xml"})
            }
            other => {
                let _ = std::fs::remove_dir_all(&tmp_dir);
                return Err(format!("Unknown export format {other:?}"));
            }
        };
        let res = openxer_automation::parse_command(req).and_then(openxer_automation::dispatch);
        let _ = std::fs::remove_dir_all(&tmp_dir);
        match res {
            Ok(_) => Ok(format!("Exported {fmt} to {dest}.")),
            Err(e) => Err(format!("Export failed: {}", e.message)),
        }
    }
}

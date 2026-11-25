# What agents can do: the app mapped to the CLI and MCP

Every app command that changes project or workspace data, or computes something a user reads, with the way an agent does the same through `openxer-cli` and its MCP server. Operations in the **Operation** column run in an `apply` batch (MCP `openxer_apply`) and through `openxer-cli edit` / `call`; the ones with their own command and tool are named in **CLI** and **MCP**. Edits go through the same functions as the app (`xer/src/edit`), so the results are the same. Rows marked **gap** are not available to agents yet. Pure view preferences (layouts, columns, filters, colors, zoom, toolbars, fonts) change no project data and are left out.

The desktop test `agent_coverage` (in `desktop/src/agent_coverage.rs`) fails when the app draws a command that is not listed in its registry, and `automation/tests/coverage.rs` fails when an operation or tool is missing from this file, the README or `--help`. Add the mapping (or a gap row here) with every new command.

## Activities, relationships and assignments

| Feature | App | CLI | MCP | Operation | Notes |
| --- | --- | --- | --- | --- | --- |
| Read activities, relationships, project options | Activities view | `load` | `openxer_load` | | paged; dates, floats, status, linked documents; `fields`, `activities`, `wbs`, `critical` and `sort` keep the answer small |
| Add, delete an activity | Edit > Add, Delete | `add-activity`, `delete-activity` | `openxer_add_activity`, `openxer_delete_activity` | `add_activity`, `delete_activity` | both take the project's Defaults tab values (types, calendar, 5 day duration); without `activity` the ID comes from the auto-numbering defaults, `selected` standing for the app's selection. In the app Add opens the New Activity wizard unless it is turned off; the activity it makes is the same one with the wizard's choices set on it |
| Activity fields (General, Status tabs, table cells, Fill Down, Replace) | Activity Details, table | `set-activity` | `openxer_set_activity` | `set_activity` | every field in README; also the Status tab units and costs (`budgeted_labor_units`, `actual_nonlabor_cost`, `this_period_labor_units`, ...), and At Complete units and costs (`at_completion_labor_units`, `at_completion_labor_cost`: the remaining becomes it less the actual) |
| Relationships (tabs, Assign Predecessors/Successors, Link/Unlink, Gantt and network drag) | Relationships tab, Edit menu | `relationship` | `openxer_relationship` | `relationship` | with lag and comments |
| Resource assignments | Resources tab, Edit > Assign > Resources | `assign`, `unassign`, `set-assignment` | `openxer_assign`, `openxer_unassign`, `openxer_set_assignment` | `assign_resource`, `remove_assignment`, `set_assignment` | units, costs (`budgeted_cost`, `actual_cost`, `remaining_cost`), At Completion (`at_completion_units`, `at_completion_cost`: the remaining takes it less the actual), Rate Source (`rate_source`: resource, role, override), Cost Units Linked (`calculate_costs_from_units`), rates, lags, curve, durations, drive dates, `resource` (replace), `role`, `cost_account`; milestones take none (`milestone_assignment`) |
| Activity codes | Codes tab, Assign > Activity Codes | | | `set_activity_code` | one value per code type; Assign with several values of one type selected takes the first, as `set_activity_code` replaces the value |
| Notebook topics | Notebook tab | | | `set_notebook` | |
| User defined field values | UDF tab; User Defined columns of the WBS, Projects, Resources and Resource Assignments windows and of the Expenses, Issues, Risks and WPs & Docs views | | | `set_udf` | every subject area: activities, resources, assignments, steps, WBS, the project, expenses, issues, risks, documents |
| Steps | Steps tab | `add-step`, `set-step`, `delete-step`, `move-step` | `openxer_add_step`, `openxer_set_step`, `openxer_delete_step`, `openxer_move_step` | `add_step`, `set_step`, `delete_step`, `move_step` | steps drive physical % when the project says so |
| Step templates on an activity | Steps tab: Add from Template, Create Template; Assign > Steps for several activities | `add-steps-from-template`, `create-step-template` | `openxer_add_steps_from_template`, `openxer_create_step_template` | `add_steps_from_template`, `create_step_template` | `after` names the step they go in after (else after the last step); several activities take one operation each in one batch |
| Feedback: notes to resources, review flag | Feedback tab | `set-feedback` | `openxer_set_feedback` | `set_feedback` | resource feedback itself comes from timesheets |
| Expenses | Expenses tab and view | `add-expense`, `set-expense`, `delete-expense` | `openxer_add_expense`, `openxer_set_expense`, `openxer_delete_expense` | `add_expense`, `set_expense`, `delete_expense` | read with `records --kind expenses` |
| Risks on an activity | Risks tab | `assign-risk`, `unassign-risk` | `openxer_assign_risk`, `openxer_unassign_risk` | `assign_risk`, `unassign_risk` | |
| Documents on an activity | WPs & Docs tab | `link-document`, `unlink-document` | `openxer_link_document`, `openxer_unlink_document` | `link_document`, `unlink_document` | |
| Typed period units (Manual curve) | Resource Usage Spreadsheet cells, Fill Down, Fill Across, Range Paste; Resources tab Curve cleared | `set-assignment --field period_remaining_units --value DATE[,DATE]=UNITS` | `openxer_set_assignment` | `set_assignment` field `period_remaining_units` (`curve` null removes the typed values) | **gap**: typed budgeted or actual period units (not observed) |
| Resource curves | Resources tab, Enterprise > Resource Curves | `curves` | `openxer_curves` | `set_assignment` field `curve` | **gap**: adding or editing a curve's points |
| Resource and activity usage (time-phased units and costs) | Activity Usage Profile and Spreadsheet, Resource Usage Profile and Spreadsheet (bottom panes) | `usage` | `openxer_usage` | | read only; same spreads as the views, curves applied |
| Cut and paste activities in their own project (a move to the target's WBS) | Edit > Cut, Paste | `set-activity` | `openxer_set_activity` | `set_activity` field `wbs` | the move keeps the Activity ID, relationships and child rows |
| Copy and paste activities | Edit > Copy, Paste | | | | **gap**: paste with child rows and the Renumber Activity IDs and Duplicates steps that give the copies their IDs (pure, `desktop/src/editing/ops.rs`: `paste_ids`, `paste_conflicts`, `paste_renumbered`); also moving activities to another project (Cut and Paste between open projects in the app) |
| Dissolve | Edit > Dissolve | `dissolve-activity` | `openxer_dissolve_activity` | `dissolve_activity` | `retain_lag` is the app's User Preferences, Calculations, Retain Lag (default off); one activity per operation |
| Renumber Activity IDs | Edit > Renumber | | | `set_activity` field `activity_id` | the numbering plan is a **gap**; stored baselines are not renumbered |
| Assign by role, role only | Edit > Assign > Resources by Role, Roles | | | | **gap** (the setter takes a role) |
| Gantt bar drag | Gantt | | | `set_activity` `start`, `original_duration` | |
| Activity critical path through one activity | View > Activity Critical Path | | | | **gap** (pure, `desktop/src/critical_path.rs`) |
| Summary tab | Activity Details | `load`, `earned-value` | `openxer_load`, `openxer_earned_value` | | |

## Project, WBS and registers

| Feature | App | CLI | MCP | Operation | Notes |
| --- | --- | --- | --- | --- | --- |
| Data date, Must Finish By | Project details (Dates tab), F9 | `set-project`, `schedule` | `openxer_set_project`, `openxer_schedule` | `set_project` | the data date goes through `xer::edit::project_dates::set_data_date`, as the Dates tab does, and schedules nothing unless `schedule` is true |
| Project details: General, Defaults, Resources, Settings, Calculations | Projects view | | | `set_project` (a PROJECT column) | values written as given; planned start is `plan_start_date`; the Defaults tab is `def_duration_type`, `def_complete_pct_type`, `def_task_type`, `acct_id`, `clndr_id`, `task_code_prefix`, `task_code_base`, `task_code_step`, `task_code_prefix_flag`; Define Critical Activities on the Settings tab is `critical_path_type` (`CT_TotFloat` or `CT_DrivPath`) and `critical_drtn_hr_cnt` (hours), the desktop writes every tab through the same `xer::edit::tables::set_project_field` |
| Schedule options | Tools > Schedule > Options | `schedule --options` | `openxer_schedule` | `set_schedule_option`, `schedule` | |
| Project funding | Projects view Funding tab | | | `add_project_funding`, `set_project_funding`, `delete_project_funding` | |
| Add WBS elements | WBS view Add | `add-wbs` | `openxer_add_wbs` | `add_wbs` | by code path |
| WBS fields (name, status, manager, dates) | WBS details; WBS table Replace (names); Projects table Fill Down (Project Name: the project node's name) | | | `set_wbs` (a PROJWBS column; `wbs` `""` is the project node) | manager by `obs_id`; the desktop writes the project name through `xer::edit::wbs::set_project_name` |
| WBS earned value techniques | WBS details Earned Value | | | `set_wbs_earned_value` | |
| WBS and project notebooks | WBS details Notebook, Project details Notebook (Add, Delete, Modify with its formatting toolbar) | | | `set_wbs_notebook` | `""` is the project node; the text is HTML as stored, which the app writes with bold, italic, underline and lists |
| Project codes on a project | Project details Codes tab (Assign, Remove) | | | `set_project_code` | one value per code type, as the app assigns them; the Project Codes dictionary itself is a **gap** |
| Move a WBS element | WBS view Shift up/down/left/right | `shift-wbs` | `openxer_shift_wbs` | `shift_wbs` | |
| WBS code | WBS details General | `rename-wbs-code` | `openxer_rename_wbs_code` | `rename_wbs_code` | unique among siblings |
| Cut, copy, paste, delete or merge WBS elements | WBS view | | | | **gap** (pure, `desktop/src/views/wbs.rs`; a copy with its notebook, milestones and documents is `xer::edit::wbs::copy_subtree`; with its activities, Copy WBS Options in `desktop/src/views/wbs_copy.rs`, which needs the activity paste of `desktop/src/editing/ops.rs`, see "Copy and paste activities") |
| Budget log, spending plan, WBS milestones | WBS and project details | | | | **gap** (pure, `desktop/src/views/budget.rs`) |
| Issues | Project > Issues | `records`, `add-issue`, `set-issue`, `delete-issue` | `openxer_records`, `openxer_add_issue`, `openxer_set_issue`, `openxer_delete_issue` | `add_issue`, `set_issue`, `delete_issue` | issue history is read only |
| Risks | Project > Risks | `records`, `add-risk`, `set-risk`, `delete-risk` | `openxer_records`, `openxer_add_risk`, `openxer_set_risk`, `openxer_delete_risk` | `add_risk`, `set_risk`, `delete_risk` | |
| Thresholds | Project > Thresholds | `records`, `add-threshold`, `set-threshold`, `delete-threshold` | `openxer_records`, `openxer_add_threshold`, `openxer_set_threshold`, `openxer_delete_threshold` | `add_threshold`, `set_threshold`, `delete_threshold` | |
| Monitor Thresholds | Tools > Monitor Thresholds, Thresholds view Monitor | `monitor-thresholds` | `openxer_monitor_thresholds` | `monitor_thresholds` | assigned baselines read from the workspace |
| Work products, documents | Project > WPs & Docs | `documents`, `read-document`, `add-document`, `set-document`, `delete-document` | `openxer_documents`, `openxer_read_document`, `openxer_add_document`, `openxer_set_document`, `openxer_delete_document` | `add_document`, `set_document`, `delete_document` | document UDF values through `set_udf` (`document`) |
| Earned value figures | EV columns, WBS and project bands | `earned-value` | `openxer_earned_value` | | planned value follows resource curves as the columns do (`xer/src/ev.rs`) |
| Baselines: list, add (copy or convert), copy, delete, restore, Baseline ID, name and type | Project > Maintain Baselines | `baseline list`, `baseline add`, `baseline convert`, `baseline copy`, `baseline delete`, `baseline restore`, `baseline set` | `openxer_baseline_list`, `openxer_baseline_edit` | | the app's rules (`automation/src/baseline_ops.rs`); delete and restore refuse an assigned baseline; restore files the new project in the owner's EPS folder |
| Assign Baselines (project, primary, secondary, tertiary) | Project > Assign Baselines | `baseline assign` | `openxer_baseline_edit` | | the project baseline is the project's `sum_base_proj_id`; the user baselines live in the workspace |
| Revert Project to Baseline | Project > Revert Project to Baseline | `baseline revert` | `openxer_baseline_edit` | | the project takes the baseline's data and keeps its ID, name and baselines |
| Update a baseline from the project (options, Preview, Update, Last Update Date) | Maintain Baselines > Update... (Update Baseline, Preview Changes) | `baseline update` (`--dry-run true` returns the preview) | `openxer_baseline_update` | | the app's rules (`automation/src/baseline_ops.rs` `update`, engine `interchange/src/baseline_update.rs`); every option of the dialog by name, defaults as the dialog's; activities matched by GUID, then Activity ID; **gap**: a saved filter is evaluated by the app only, the CLI takes the Activity IDs (`activities`); Compare all records and Run optimized are accepted and have no effect |

## Tools

| Feature | App | CLI | MCP | Operation | Notes |
| --- | --- | --- | --- | --- | --- |
| Schedule (F9), level resources | Tools > Schedule, Level Resources | `schedule` | `openxer_schedule` | `schedule` | leveling options through `set_schedule_option`; the leveling log is a **gap** |
| Update Progress | Tools > Update Progress | `update-progress` | `openxer_update_progress` | `update_progress` | all or named activities; flagged expenses of what it progresses are spent; **gap**: the spotlight's highlight is the app's (the CLI takes Activity IDs) |
| Apply Actuals | Tools > Apply Actuals | `apply-actuals` | `openxer_apply_actuals` | `apply_actuals` | the shared rules (`xer/src/edit/progress.rs`): activity, resource and expense Auto Compute Actuals flags, assignment units and costs, actual this period; one data date for the project (the app's per-project dates are one call per project) |
| Auto Compute Actuals on an activity | Activities table, General column Auto Compute Actuals (check box) | `set-activity` field `auto_compute_actuals` | `openxer_set_activity` | `set_activity` | the app's cell is the shared activity edit `Edit::AutoComputeActuals` |
| Recalculate actual units and cost when duration % changes | Projects, Calculations tab | `set-project` field `act_pct_link_flag` | **gap**: `openxer_set_project` takes only the data date and must finish | `set_project` | read by every Duration % edit (`xer/src/edit/activity.rs`, so `set-activity` `duration_percent_complete` follows it), on in a new project |
| Recalculate Assignment Costs | Tools > Recalculate Assignment Costs | `recalculate-costs` | `openxer_recalculate_costs` | `recalculate_costs` | the current project |
| Top Down Estimation | Tools > Top Down Estimation | `top-down-estimate` | `openxer_top_down_estimate` | `top_down_estimate` | |
| Store Period Performance | Tools > Store Period Performance | | | | **gap** (pure, `desktop/src/enterprise/ops.rs`) |
| Global Change | Tools > Global Change (Modify Global Change, Global Change Report) | `global-change` | `openxer_global_change` | `global_change` | a definition saved in the workspace input (`name`) or given as `rule`; the same definitions, If evaluation (All or Any, "is under" for WBS and activity codes), Then and Else statements (user defined fields included) and writes as the app (`xer/src/edit/global_change.rs` over the shared field catalog `xer/src/fields/`); a dry run returns the report lines (activity, field, old, new, exception), otherwise the lines without an exception are written and counted. Started from an Activities view, the app changes only the activities its filter shows; an agent puts those activities in the If rows (there is no window filter outside the app). Relative dates in If values (the app's filter keywords) and the table-only columns (baselines, earned value and the other derived columns, notebook topics, relationship details) are a **gap** outside the app |
| Check Schedule | Tools > Check Schedule | `check-schedule` | `openxer_check_schedule` | | the results rows with the default or given hour thresholds, and one row's Activity IDs or relationships; the same rules as the dialog (`xer/src/schedule_check.rs`); Late Activities and BEI compare with the workspace's assigned project baseline (or the project's own dates when there is none); writing the HTML report is a **gap** |
| Project Checker | Tools > Project Checker | | | | **gap** (pure, `desktop/src/reports/checker.rs`) |
| Schedule Comparison | Tools > Schedule Comparison | `compare` | `openxer_compare` | | **gap**: which activities match, were added or deleted, and the logic changes are one shared core (`xer/src/compare.rs`) the app and `compare` both call; the field-by-field changes are still read twice: the app through its column display (every column, per-area choices, report formats), `compare` directly from the project for dates, durations, constraints, calendars, names and progress |
| Float paths into an activity | View > Activity Critical Path | `paths` | `openxer_paths` | | the scheduler's multiple float paths ending at the activity (or the project finish), with the gap behind path 1, merge points and the critical counts |
| Look-ahead | the 3-Week Lookahead filter and layout | `lookahead` | `openxer_lookahead` | | read only; each activity in the window is in progress, ready or waiting, with what on |
| Milestones, schedule statistics, resource loading, WBS rollup | Activities view columns and bands, usage profiles | `milestones`, `schedule-stats`, `resource-loading`, `wbs-summary` | `openxer_milestones`, `openxer_schedule_stats`, `openxer_resource_loading`, `openxer_wbs_summary` | | read only; numbers the agent skills report so an agent never computes them |
| Skills for AI agents | AI Agents > Skills | `skills list`, `skills show`, `skills validate`, `skills new`, `skills path` | `openxer_skills`, `openxer_skill`, MCP prompts | | built-ins from `assets/skills/`, the user's from `<data folder>\skills\` |
| S-curve export | Tools > Reports > S-Curve | `export-scurve` | `openxer_export_scurve` | | same figures and workbook as the app (`xer/src/scurve.rs`, `interchange/src/scurve_xlsx.rs`); the app can also limit it to the current activity filter |
| Cumulative earned hours export | Tools > Reports > Earned Hours | `export-earned-hours` | `openxer_export_earned_hours` | | same figures and workbook as the app |
| Reports, report wizard, report groups, batch reports | Tools > Reports | | | | **gap**: the report engine runs on desktop lookups; definitions live in the workspace |
| Circular relationship analysis | after F9 | | | `relationship` to fix | the analysis is a **gap** (pure, `desktop/src/loops/analysis.rs`) |
| Print | File > Print | `export-pdf` | `openxer_export_pdf` | | runs the desktop renderer headless |

## Import, export and the workspace

| Feature | App | CLI | MCP | Operation | Notes |
| --- | --- | --- | --- | --- | --- |
| Find the user's schedules (the workspace and its projects, a folder's XER files) | File > Recent Projects | `find-schedules` | `openxer_find_schedules` | | read only; for an agent not given a file |
| Inspect a file or workspace | | `inspect` | `openxer_inspect` | | |
| Import XER (new, update, replace, per-type actions) | File > Import | `import` | `openxer_import` | | undo last import is a **gap** |
| Import project XML | File > Import, Project - (XML) | `import` (XML input, `--format`) | `openxer_import` (`format`) | | same actions as XER; read into the same tables, including UDF values, notebook topics and notes, resource codes, OBS, currencies and schedule options; checked against an export of the reference; `inspect` lists the element types and fields it skips |
| Export XER | File > Export | `export` | `openxer_export` | | export with stored baselines is a **gap** |
| Export project XML | File > Export, Project - (XML) | `export --format xml` (or a `.xml` output) | `openxer_export` (`format`) | | writes the structure of the reference's export (root, child order, value forms); not yet opened in the reference; export with stored baselines is a **gap**, as for XER |
| Excel export and import | File > Export, Import | `export-xlsx`, `import-xlsx` | `openxer_export_xlsx`, `openxer_import_xlsx` | | saving export templates is a **gap** |
| Move to a shared workspace dictionary (schema 3), or upgrade an earlier schema 2 | Admin > Share Dictionaries Across Projects (opening upgrades schema 2) | `workspace migrate` | `openxer_workspace_migrate` | | the CLI writes a migrated copy; the app converts in place after a backup |
| List the shared workspace dictionary and its uses | | `workspace dictionary`, `workspace dictionary-usage` | `openxer_workspace_dictionary`, `openxer_workspace_dictionary_usage` | | read only |
| New Activity and New Resource wizards on or off | Edit > User Preferences, Assistance, Add activities through the wizard and Add resources through the wizard; each wizard's own "Skip this wizard next time" | `workspace preference` | `openxer_workspace_preference` | | reads the switches, or writes a copy of the workspace with `new_activity_wizard` or `new_resource_wizard` set; the app reads them at start. The wizards themselves are the app's: agents add activities with `add_activity` and resources with `add_resource`, then set their fields |
| EPS folders, filing, a new project | Enterprise > EPS, File > New, Projects view | `eps list`, `eps ...` | `openxer_eps_list`, `openxer_eps_edit` | | a new project gets an internal project number no workspace project or baseline uses (`baseline_store::next_project_number`), as File > New, Paste and baseline restore do; EPS responsible manager and sibling order are a **gap** |
| Delete an EPS folder with the folders and projects under it (and their baselines) | Enterprise > EPS > Delete, Projects view Delete on a folder row | `eps delete-folder --delete-projects` | `openxer_eps_edit` (`delete_folder`, `delete_projects`) | | a folder holding projects is refused without the flag; `--dry-run` counts what goes; the app's Undo has no CLI counterpart, the untouched input is the way back |
| Copy or delete a project | Projects view | | | | **gap** |

## Enterprise and admin dictionaries

| Feature | App | CLI | MCP | Operation | Notes |
| --- | --- | --- | --- | --- | --- |
| Resources: add, fields, parent, calendar, delete, merge | Enterprise > Resources; Resources table Fill Down and Replace (ID, name) | | | `add_resource`, `set_resource`, `delete_resource`, `merge_resources` | copy/paste a resource is a **gap** |
| Resource prices and limits | Resources view Units & Prices | | | `set_resource_limit` | |
| Resource roles and codes | Resources view Roles, Codes tabs | | | | **gap** (pure, `desktop/src/views/resource_edit.rs`) |
| Calendars: work weeks and exceptions | Enterprise > Calendars | `calendar-day`, `calendar-weekday` | `openxer_calendar_day`, `openxer_calendar_weekday` | `calendar_day`, `calendar_weekday` | |
| Calendars: add, copy, rename, default, convert, delete, fields | Enterprise > Calendars | `copy-calendar`, `rename-calendar`, `set-default-calendar`, `convert-calendar` | `openxer_copy_calendar`, `openxer_rename_calendar`, `openxer_set_default_calendar`, `openxer_convert_calendar` | `add_calendar`, `copy_calendar`, `rename_calendar`, `set_default_calendar`, `convert_calendar`, `delete_calendar`, `set_calendar` | `set_calendar` writes hours per period and other columns as given; `delete_calendar` with `replacement` or `default_global` is the Calendars in Use choice (Select Replacement Calendar, Use Default Global Calendar) |
| Activity codes dictionary | Enterprise > Activity Codes; Assign Activity Codes, New... (Add Code Value) | | | `add_code_type`, `add_code_value`, `set_code_value`, `delete_code_value` | renaming, reordering or deleting a code type is a **gap** |
| Project, resource, role and assignment codes | Enterprise menu | | | | **gap** (pure, `desktop/src/dictionaries/ops.rs`) |
| User defined fields dictionary | Enterprise > User Defined Fields | | | | **gap**; values are covered by `set_udf` |
| Roles, OBS, shifts, funding sources, cost accounts, step templates dictionary | Enterprise menu | | | | **gap** (pure, `desktop/src/enterprise/ops.rs`, `dictionaries/ops.rs`, `dictionaries/steps.rs`) |
| Financial periods: delete | Admin > Financial Period Calendars, Delete Period | `apply` | `openxer_apply` | `delete_financial_period` | refused, nothing changed, while stored period performance refers to the period (`xer/src/edit/financial_periods.rs`, shared with the app) |
| Admin categories, currencies, financial period calendars, adding and renaming periods, Batch Create | Admin menu | | | | **gap** (pure) |

## The Chat window

| Feature | App | CLI | MCP | Operation | Notes |
| --- | --- | --- | --- | --- | --- |
| Ask about the open project | Chat window (Ask) | `mcp --toolset chat --mode ask` | `openxer_load`, `openxer_records`, `openxer_earned_value`, `openxer_usage`, `openxer_documents`, `openxer_check_schedule`, `openxer_playbook` | | the chat tool set (`automation/src/chat_tools.rs`): the same tools with short descriptions, the snapshot as input, results under 40,000 bytes; hosted agents run through `automation/src/agent_host/` |
| Scheduling methods (build, update, audit, critical path, delay recovery, schedule check) | Chat window | `playbook` | `openxer_playbook` | | step-by-step playbooks the assistant loads when needed (`assets/playbooks/`); the chat's instructions are `chat_prompt::system_prompt`, with the user's own `chat\AGENTS.md` in the data folder |
| Propose edits to the open project | Chat window (Edit) | `mcp --toolset chat --mode edit` | `openxer_apply` (propose only) | any `apply` batch | always a dry run on the snapshot; the app applies an approved batch to the open project with `edit::apply_changes` |

## Not for agents

Open, save, close, undo and redo, the clipboard, Refresh, Exit, Select All, Find, Spell Check, Set Default Project, user preferences (except the Assistance switches above), the AI Agents menu and Help belong to the running app (the menu's config work is `openxer-cli connect`, and its Copy Setup Prompt is `connect prompt`): every automation command reads its input and writes a new output, and agents use the edits above.

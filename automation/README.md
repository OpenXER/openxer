# OpenXER automation

`openxer-cli.exe` gives native scheduling and editing through a command line and an MCP stdio server. Both call the same `openxer-automation` dispatcher, which uses the `openxer-xer` edit layer and the `openxer-engine` scheduler. Edits go through the same setters as the desktop app, so the app, the CLI and MCP always give the same result. No Python runtime and no other scheduling application is needed.

Inputs are XER files or OpenXER workspace databases (`.db`, `.sqlite`, `.sqlite3`). Reads never change the input. Every command that changes or exports data needs an explicit **new** output path; an existing destination is refused. Output is prepared in the destination folder and published without overwriting another file.

## Contents

- [CLI](#cli)
- [Editing: apply and the single edits](#editing-apply-and-the-single-edits)
- [Scheduling](#scheduling)
- [Relationships](#relationships)
- [Import, export, Excel and PDF](#import-export-excel-and-pdf)
- [Documents](#documents)
- [Expenses, issues, risks and thresholds](#expenses-issues-risks-and-thresholds)
- [Earned value](#earned-value)
- [Progress, steps and the Tools edits](#progress-steps-and-the-tools-edits)
- [Global Change](#global-change)
- [Schedule analysis](#schedule-analysis)
- [Skills for AI agents](#skills-for-ai-agents)
- [Coverage map](COVERAGE.md)
- [Project folders (EPS) and rollups](#project-folders-eps-and-rollups)
- [Baselines](#baselines)
- [JSON contract](#json-contract)
- [MCP](#mcp)
- [Chat agent host](#chat-agent-host)
- [In-process use and tests](#in-process-use-and-tests)

## CLI

Build from the repository root with `cargo build --release -p openxer-automation`. The Windows release package puts `openxer-cli.exe` beside `OpenXER.exe`.

```powershell
.\openxer-cli.exe inspect --input C:\Schedules\plan.xer
.\openxer-cli.exe import --input C:\Schedules\plan.xer --output C:\Schedules\workspace.db
.\openxer-cli.exe load --input C:\Schedules\workspace.db --project 101 --offset 0 --limit 200
.\openxer-cli.exe set-activity --input C:\Schedules\workspace.db --project 101 --activity A1010 --field remaining_duration --value 24 --output C:\Schedules\edited.db
.\openxer-cli.exe apply --input C:\Schedules\edited.db --project 101 --changes C:\Schedules\week-14.json --output C:\Schedules\updated.db
.\openxer-cli.exe schedule --input C:\Schedules\updated.db --project 101 --output C:\Schedules\scheduled.db --data-date "2026-04-06 08:00" --level
.\openxer-cli.exe export --input C:\Schedules\scheduled.db --project 101 --output C:\Schedules\scheduled.xer
.\openxer-cli.exe --help
```

Commands:

| Command | What it does |
| --- | --- |
| `find-schedules` | Names the OpenXER workspace on this PC with its projects, most recently saved first, and with `--folder` the schedule files in one folder (read only; see MCP) |
| `inspect` | Lists the projects in a file (read only); for project XML also the element types read (`xml.elements`), the ones OpenXER has no place for and skipped (`xml.unhandled`) and the fields it does not read (`xml.fields_not_read`) |
| `load` | Pages a project's options, activities and relationships (read only) |
| `curves` | Lists the resource curves an assignment can use, with their points (read only) |
| `usage` | An activity's or a resource's assignments spread per day, week or month: budgeted, actual, remaining and remaining late units or costs, with curves (read only) |
| `import` | Imports XER or project XML projects into a new workspace, or into a copy of an existing one |
| `schedule` | Runs the scheduler, with optional resource leveling |
| `relationship add\|update\|remove` | Edits one relationship; dates are kept until the next `schedule` |
| `apply` | Applies a batch of edits (`openxer-changes/1`), all or nothing |
| `set-activity`, `add-activity`, `delete-activity`, `dissolve-activity`, `assign`, `unassign`, `set-assignment`, `calendar-day`, `calendar-weekday`, `set-project`, `add-wbs`, `add-document`, `set-document`, `delete-document`, `link-document`, `unlink-document`, `add-expense`, `set-expense`, `delete-expense`, `add-issue`, `set-issue`, `delete-issue`, `add-risk`, `set-risk`, `delete-risk`, `assign-risk`, `unassign-risk`, `add-threshold`, `set-threshold`, `delete-threshold`, `monitor-thresholds`, `add-step`, `set-step`, `delete-step`, `move-step`, `add-steps-from-template`, `create-step-template`, `set-feedback`, `update-progress`, `apply-actuals`, `recalculate-costs`, `top-down-estimate`, `shift-wbs`, `rename-wbs-code`, `rename-calendar`, `set-default-calendar`, `convert-calendar`, `copy-calendar` | One edit each, the same operations as `apply` |
| `documents` | Lists a project's work products and documents with their links (read only) |
| `read-document` | Returns the text of the file a document points at, paged (read only) |
| `export` | Writes a new XER, or project XML |
| `export-xlsx` | Writes a new Excel workbook |
| `import-xlsx` | Previews or applies a spreadsheet of changes |
| `export-pdf` | Prints a layout to a new PDF |
| `workspace migrate` | Previews (`--dry-run`) or writes (`--output NEW.db`) a copy of a workspace that keeps shared calendars, resources, codes and the other workspace dictionaries once |
| `workspace dictionary` | Lists a workspace's shared dictionary, each row with the projects that use it (read only) |
| `workspace dictionary-usage` | Says which projects use one dictionary row (read only) |
| `workspace preference` | Reads the User Preferences, Assistance switches kept in a workspace (`new_activity_wizard`, on by default, decides whether Edit > Add opens the New Activity wizard; `new_resource_wizard`, on by default, the same for the Resources view's Add and the New Resource wizard), or with `--name` and `--value` writes a copy (`--output NEW.db`) with one set; `--dry-run` previews |
| `call` | Runs one complete JSON request (the same schema as MCP) |
| `mcp` | Serves MCP over stdio |

`--project` is the project id (the XER `proj_id`); `--workspace-id` is the OpenXER workspace row id, for when project ids repeat. When several rows match, the command refuses the ambiguity instead of choosing one. XER import takes every project unless `--project` is given.

### Workspaces and the shared dictionary

A workspace keeps its dictionaries once for all of its projects: global calendars, resources with their rates, roles, global and EPS activity codes with their values, UDFs, cost accounts, responsible managers, currencies, curves, shifts, funding sources and the admin categories (workspace schema 3). Each project's stored text holds only its own rows (its WBS, activities, relationships, assignments, project calendars and project codes); every command that reads a workspace project adds the shared rows back, so `load`, `schedule`, `apply`, the edits and the exports see one whole project, and every command that writes one merges its shared rows back into the dictionary. An edit of a global calendar or a resource through any command therefore reaches every project of the workspace. An EPS activity code is seen by the projects of its EPS node and of the nodes below it; one that comes with an XER file is filed under the importing project's node. `inspect` reports `workspace_schema` and, for schema 3, the dictionary's revision and row counts. New workspaces (`import` to a new file, `save` of an XER to a new `.db`) share their dictionary from the start; a workspace made by an earlier version (schema 1) keeps each project's whole text and is read and written as before until it is converted. A workspace an earlier version gave a shared dictionary without the EPS codes (schema 2) gives `workspace_needs_migration` until `workspace migrate` upgrades it (the app upgrades it in place, after a backup, when it opens it).

`workspace migrate --input workspace.db --dry-run` converts a schema 1 workspace, or upgrades a schema 2 one: it reports, as JSON with a plain-text `report`, how the projects would share one workspace dictionary: rows that are the same in several projects become one, a later row whose name is taken by different content is kept as `<name> (<project>)` and listed with the fields that differ, and ids already taken are renumbered with every reference rewritten. `--output NEW.db` writes the migrated copy; the input is not changed and stays as its backup. Before anything is written every project is checked: hydrated from the new dictionary it must hold all of its rows and schedule, unleveled and leveled, to the same dates and float as before, or the command fails with `migration_failed`. The app's `Admin > Share Dictionaries Across Projects...` does the same in place, after a backup.

`workspace dictionary --input workspace.db [--table calendar]` lists the dictionary: per table, each row's `id`, `name` and `used_by` (the workspace ids of the projects that use it; calendars add `type` and `default`, code types `scope` and `eps_node`). `workspace dictionary-usage --input workspace.db --table resource (--id N | --name NAME)` answers for one row with the projects' ids and names. `--table` takes `calendar`, `resource`, `role`, `activity_code`, `code_value`, `udf`, `account`, or a table name such as `RSRCRATE`. A schema 1 workspace gives `workspace_needs_migration`.

`import --workspace` merges each file project's dictionary rows into the workspace's: a row with the same name and content is the workspace's own (importing the same file twice adds nothing), a row whose name the workspace has with different content is added as a copy `<name>_<n>`, and anything else is added. `--dictionary KIND=keep|update|copy,...` chooses otherwise per kind (`calendar=keep,resource=update`): `keep` uses the workspace's row, `update` gives the workspace's row the file's values for every project that uses it. Each project in the result lists these decisions in `dictionary`, with the fields that differ. `export` writes only the dictionary rows its projects use.

`load` pages activities (`--offset`, `--limit`) and relationships (`--relationship-offset`, `--relationship-limit`) independently. Each limit defaults to 200 and allows 0 to 2000 (0 returns no rows of that kind, only the counts, and no next offset), and the response gives both continuation offsets. To keep an answer small, `--fields` names the activity fields to return (`task_code` is always kept; `wbs`, the code path, and `critical`, by the project's own definition, are given only when named), and `--activities ID,ID`, `--wbs PATH` (the element and everything under it) and `--critical` keep only the matching activities; then the relationships are those with an end among them, `matched_activities` and `matched_relationships` count them, and unknown Activity IDs come back in `not_found`. `--sort FIELD` orders the activities by a field, `-FIELD` descending (`--sort=-early_finish` lists the latest finish first; WBS paths sort element by element, numbers by value; empty values go last, equal values keep the project's order). MCP takes the same as `fields`, `activities` (arrays), `wbs`, `critical` and `sort`. `fields` also names `original_duration_minutes`, `actual_start`, `actual_finish`, `constraint` and `constraint_date`, given only when named.

`check-schedule` runs Tools > Check Schedule read-only, with the app's own rules (`xer/src/schedule_check.rs`, which the dialog runs too) and its default targets: one row per check (Logic, Negative lags, Positive Lags, Long Lags, Relationship Types, Hard Constraints, Soft Constraints, Large Float, Negative Float, Large Durations, Activities with Invalid progress dates, Resources / Cost, Late Activities, BEI, Out Of Sequence) with `check`, `count`, `of`, `result` (a percent, or the index for BEI), `target`, `pass` and `first` (its first five offenders), and the hour thresholds. `--check NAME` also lists that row's Activity IDs (relationships as `PRED -> SUCC TYPE`), `--limit` of them (default 50, at most 500). Late Activities and BEI compare with the project's own planned finishes: the command reads no project baseline.

`playbook` lists the scheduling playbooks (`build`, `update`, `audit`, `critical-path`, `delay-recovery`, `schedule-check`) and `--name` returns one: a step-by-step method, written for a working scheduler, that names the chat tools and load options to use at each step. They are compiled in from `assets/playbooks/`. Activity codes are exact and case sensitive. Each activity row lists the documents linked to it (`documents`: `document_id`, `title`, `reference`, `work_product`).

`call --json '<request>'` or `call --request-file request.json` runs one complete request, the CLI arguments as a JSON object with a `command` field:

```json
{
  "command": "schedule",
  "input": "C:\\Schedules\\workspace.db",
  "output": "C:\\Schedules\\scheduled.db",
  "project_id": 101,
  "options": { "data_date": "2026-04-06 08:00", "level_resources": true }
}
```

## Editing: apply and the single edits

Edits name things the way you see them in the app:

- activities by Activity ID, resources by short name, calendars by name, code types and user-defined fields by their names, and WBS elements by their code path (`1.2.3`);
- durations and lags in **hours**, dates as project-local `YYYY-MM-DD HH:MM` (or `YYYY-MM-DD`);
- choices as XER codes (`TT_Task`, `CS_ALAP`, `DT_FixedDUR2`) or display names;
- `null` clears a value.

They run through the desktop's setters, so statuses, durations, units and assignments follow exactly as they would when typed in the app. An edit to a started activity moves its status as the app would, and a duration type decides which of duration, units and units per hour follows a change.

### apply

```powershell
.\openxer-cli.exe apply --input C:\Schedules\workspace.db --project 101 --changes C:\Schedules\week-14.json --dry-run
.\openxer-cli.exe apply --input C:\Schedules\workspace.db --project 101 --changes C:\Schedules\week-14.json --output C:\Schedules\updated.db
```

A changes file:

```json
{
  "format": "openxer-changes/1",
  "recalculate": true,
  "operations": [
    {"op": "set_project", "field": "data_date", "value": "2026-04-06 08:00"},
    {"op": "set_activity", "activity": "A1010", "field": "actual_start", "value": "2026-03-30 08:00",
     "meta": {"source": "field update sheet, week 14"}},
    {"op": "set_activity", "activity": "A1010", "field": "remaining_duration", "value": 16},
    {"op": "add_activity", "activity": "A1015", "name": "Weld tie-ins to existing header", "wbs": "1.2"},
    {"op": "relationship", "action": "add", "predecessor": "A1010", "successor": "A1015", "kind": "FS", "lag_hours": 0},
    {"op": "assign_resource", "activity": "A1015", "resource": "PF"},
    {"op": "set_assignment", "activity": "A1015", "resource": "PF", "field": "budgeted_units", "value": 120},
    {"op": "schedule", "level_resources": false}
  ]
}
```

Operations run in file order, all or nothing: when one fails, nothing is written and the error names the operation. `--dry-run` returns the field diff (old and new values per table and row) without writing. The diff covers every table the operations write: activities, assignments, relationships, WBS, calendars, documents and their links, the project row, schedule options, resources and their limits, code types, code values and activity codes, notebook topics and notebooks, user defined field values, funding, expenses, issues and their history, risks and their activity assignments, thresholds, steps, step templates, notes to resources, feedback and curves. The result lists each operation's outcome, the diff and, for `schedule` operations, how many activities changed. An operation that writes nothing (a `set_notebook` without `text`, a `set_udf` value with no activity or resource) says why in its result, and the report lists it under `warnings`.

Every operation may carry a `meta` object (provenance such as source, evidence or who wrote it). It is accepted and ignored, never written. Any other unknown field is refused. Other top-level keys of the file (notes, a source) are carried for people and never applied.

Operations:

| Operation | Fields |
| --- | --- |
| `set_project` | `field` (`data_date` or `must_finish`), `value`, optional `schedule` (true schedules at the new data date right away) |
| `add_wbs` | `wbs` (code path), optional `name`; adds the elements of the path that do not exist yet |
| `add_activity` | optional `activity`, `selected`, `name`, `wbs`; the new activity takes the project's Defaults tab values as the app's Add does: activity type, duration type, percent complete type and calendar, 5 days of original and remaining duration, named `New Activity` unless `name` is given. Without `activity` its ID comes from the auto-numbering defaults (`task_code_prefix`, `task_code_base`, `task_code_step`, `task_code_prefix_flag`): with the flag `Y` and `selected` naming an activity, that ID plus the increment, stepping past IDs in use; otherwise the highest ID with the prefix plus the increment, or prefix and suffix when there is none. Without `wbs` it goes in the WBS of `selected`, else the project node. It starts at the data date on its calendar (finish = start + duration, kept as its duration or calendar changes), without scheduling the project; it has no late dates or float until `schedule`; the result names the ID it got |
| `delete_activity` | `activity`; its relationships and assignments go with it |
| `dissolve_activity` | `activity` (with at least one predecessor and one successor), optional `retain_lag` (default `false`); as Edit > Dissolve: each predecessor is tied to each successor, the new relationship starting where the predecessor relationship starts and ending where the successor relationship ends (A SS B and B FF D give A SF D); a pair already related keeps its relationship; lag 0, or with `retain_lag` the two lags added; no reschedule |
| `set_activity` | `activity`, `field`, `value`, optional `recalculate` |
| `assign_resource`, `remove_assignment` | `activity`, `resource`; milestones take no assignments (`milestone_assignment`) |
| `set_assignment` | `activity`, `resource`, `field`, `value`, optional `recalculate` |
| `relationship` | `action` (`add`, `edit`, `remove`), `predecessor`, `successor`, `kind` (`FS`, `SS`, `FF`, `SF`), optional `lag_hours`, optional `comments` |
| `calendar_day` | `calendar`, `date`, `shifts` (`[["08:00","12:00"],...]`, `[]` for nonwork, `null` for the weekday pattern) |
| `calendar_weekday` | `calendar`, `weekday` (0 is Sunday), `shifts` (`[]` is nonwork) |
| `schedule` | optional `level_resources` (left out, the loaded setting stays) |
| `set_resource_limit` | `resource`, `from` (date), optional `shift_period`, `max_units_per_hour` (`null` removes the row), `price_per_unit` to `price_per_unit_5` |
| `set_activity_code` | `activity`, `code_type`, `value` (code short name, `null` removes it; left out, nothing changes) |
| `set_project_code` | `code_type` (a project code), `value` (value short name, `null` removes it; left out, nothing changes); one value per code type, a new one replaces the old |
| `add_code_type` | `code_type`, optional `scope` (`project`, `eps`, `global`), optional `max_length` |
| `add_code_value`, `delete_code_value` | `code_type`, `code` |
| `set_code_value` | `code_type`, `code`, `field`, `value` (`parent` by short name, or an ACTVCODE column) |
| `set_notebook` | `activity`, `topic`, `text` (`null` removes it; left out, nothing changes) |
| `set_wbs_notebook` | `wbs`, `topic`, `text` (as `set_notebook`) |
| `set_udf` | `udf` (label), `value`, and one record: `activity`, `resource`, `activity` with `resource` (an assignment), `activity` with `step` (number from 1 or name), `wbs` (path), `project: true` (the open project), `expense`, `issue`, `risk` or `document`; a code-list field takes the code; a batch replaying a stored value whose record is gone reports it and writes nothing, a single `edit` refuses it |
| `add_calendar` | `calendar` |
| `delete_calendar` | `calendar`; a calendar in use needs `replacement` (a global calendar's name) or `default_global: true`, and everything that used it (activities, projects, resources, calendars inheriting from it) moves there before it is deleted; the default global calendar is never deleted |
| `set_calendar` | `calendar`, `field`, `value` (`base_calendar` by name, `project_calendar`, or a CALENDAR column) |
| `add_resource` | `resource` |
| `set_resource` | `resource`, `field`, `value` (`parent` by short name, `calendar` by name, or an RSRC column) |
| `set_wbs` | `wbs`, `field`, `value` (a PROJWBS column) |
| `set_schedule_option` | `field`, `value` (a SCHEDOPTIONS column) |

`set_project` (fields other than the dates), `set_calendar`, `set_resource`, `set_wbs`, `set_schedule_option` and `set_code_value` write a column as given. The column must be one the file's table holds or one the format defines for that table; another name is refused with the list of fields that can be written, so a misspelling never adds a column to the saved file. Keys and parent keys are refused: `proj_id`, `wbs_id`, `rsrc_id`, `actv_code_id`, `actv_code_type_id`, `schedoptions_id`; `parent_wbs_id` (move the element with `shift_wbs`), `parent_rsrc_id` and `parent_actv_code_id` (use `parent`), and a calendar's `base_clndr_id` and `proj_id` (use `base_calendar` and `project_calendar`).
| `add_project_funding` | `source` (a funding source name), optional `amount`, optional `share` |
| `set_project_funding` | `source`, `field` (`source`, `amount`, `share`), `value` |
| `delete_project_funding` | `source` |
| `delete_resource` | `resource` (no assignments, no child resources) |
| `delete_financial_period` | `period` (its name), optional `calendar` (the financial period calendar's name, when two periods share a name); refused while stored period performance of an activity or assignment refers to it |
| `global_change` | `name` (a definition saved in the workspace input) or `rule` (`match`, `conditions`, `then`, `otherwise`); see [Global Change](#global-change) |
| `merge_resources` | `from` (short names), `into`; assignments, rates, codes, UDF values, role links, child resources and the calendar move to `into` |
| `set_wbs_earned_value` | `wbs`, optional `performance`, `custom_percent`, `estimate_to_complete`, `performance_factor` |
| `add_document`, `set_document`, `delete_document`, `link_document`, `unlink_document` | see [Documents](#documents) |
| `add_expense` | `activity`, optional `name`, optional `fields` |
| `add_issue`, `add_risk` | optional `name`, optional `fields` |
| `add_threshold` | optional `parameter`, optional `fields` |
| `set_expense`, `set_issue`, `set_risk`, `set_threshold` | `expense`, `issue`, `risk` or `threshold` (id, or a unique name), `field`, `value` |
| `delete_expense`, `delete_issue`, `delete_risk`, `delete_threshold` | the record |
| `assign_risk`, `unassign_risk` | `risk`, `activity` |
| `monitor_thresholds` | optional `from`, `to` (one window for every threshold), `today` |
| `update_progress` | `data_date`, optional `activities` (Activity IDs), optional `remaining` (`always`, `duration_type`) |
| `apply_actuals` | `data_date`, optional `remaining` |
| `recalculate_costs` | optional `active_only`, `sync_overtime` |
| `top_down_estimate` | `labor_units`, optional `wbs`, `not_started_only` (default true), `weights` (`{"1.2": 3}`) |
| `add_step` | `activity`, `name`, optional `at` (1 is first) |
| `set_step` | `activity`, `step` (number from 1, or name), `field` (`name`, `complete`, `percent_complete`, `weight`, `description`), `value` |
| `delete_step` | `activity`, `step` |
| `move_step` | `activity`, `step`, `to` |
| `add_steps_from_template` | `activity`, `template` (name), optional `after` (step number or name; the steps go in after it, else after the last step) |
| `create_step_template` | `activity`, `name` |
| `set_feedback` | `activity`, optional `notes` (`null` clears), optional `review_required` |
| `shift_wbs` | `wbs`, `direction` (`up`, `down`, `left`, `right`) |
| `rename_wbs_code` | `wbs`, `code` |
| `copy_calendar` | `calendar` (the new name), optional `source`, optional `kind` (`project`, `global`, `resource`) |
| `rename_calendar` | `calendar`, `name` |
| `set_default_calendar` | `calendar` (global) |
| `convert_calendar` | `calendar`, `to` (`global` for a project calendar; `shared` or `personal` for a resource calendar) |

`set_activity` fields are the app's column keys: `activity_id`, `activity_name`, `wbs`, `calendar`, `activity_type`, `activity_status`, `duration_type`, `percent_complete_type`, `activity_priority`, `primary_resource`, `original_duration`, `remaining_duration`, `actual_duration`, `at_completion_duration`, `actual_start`, `actual_finish`, `start`, `start_without_constraint`, `finish`, `planned_start`, `planned_finish`, `expected_finish`, `suspend_date`, `resume_date`, `physical_percent_complete`, `duration_percent_complete`, `activity_percent_complete`, `primary_constraint`, `primary_constraint_date`, `secondary_constraint` and `secondary_constraint_date`.

On an activity that has not started, `start` moves Start and Planned Start to the date (the finish follows by the duration) and holds it there with a Start On or After constraint on that date, as answering Yes in the app does; `start_without_constraint` moves the same dates with no constraint, as answering No does (the next schedule may move it back). `finish` keeps the start and recalculates the Original and Remaining Duration to fit, with no constraint. On a started activity `start` and `start_without_constraint` set the actual start.

Milestones (`TT_Mile`, `TT_FinMile`) carry no resource or role assignments, in the app as here: `assign_resource` on a milestone is refused with the code `milestone_assignment`, and `set_activity activity_type` to a milestone is refused while the activity has assignments (the message names them; remove them first, then change the type), with `--recalculate false` too. A milestone may have a primary resource (any resource of the file), expenses, documents and work products. When a milestone becomes a task its primary resource is cleared unless it is one of the task's assignments, since a task's primary resource must be assigned. A file that already holds assignments on milestones loads and saves them untouched; the app's Project Checker lists them as "Milestone with resource assignments".

`set_activity` also takes the Status tab's units and costs, through the same setter as the tab: `budgeted_`, `actual_`, `remaining_`, `at_completion_` or `this_period_` followed by `labor_units`, `nonlabor_units` (hours), `labor_cost`, `nonlabor_cost` or `material_cost`. An At Completion value sets the remaining units (or cost) to it less the actual, so the budget follows only on a not started activity whose project links budget and remaining; a completed activity's At Completion is fixed. As in the app, a cost needs an assignment of that resource type with a price, and units go to the assignments or, without one, to the activity's own quantities.

`set_assignment` fields: `budgeted_units`, `remaining_units`, `budgeted_units_per_hour`, `remaining_units_per_hour`, `actual_regular_units`, `actual_overtime_units`, `this_period_units`, `budgeted_cost`, `remaining_cost`, `actual_regular_cost`, `actual_overtime_cost`, `this_period_cost`, `at_completion_units`, `at_completion_cost` (the remaining takes the value less the actual), `price_per_unit`, `rate_type`, `rate_source` (`resource` or `role` take that rate, `override` keeps the price to be typed), `overtime_factor`, `lag_hours`, `planned_lag_hours`, `original_duration` and `remaining_duration` (hours), `drive_activity_dates` (true or false), `resource` (another resource's short name: replace it), `role` and `cost_account` (by short name or name; `null` clears) and `curve`.

`curve` takes a resource curve's name, as `curves` lists it (case ignored), or `null` to clear: `{"op":"set_assignment","activity":"A1015","resource":"PF","field":"curve","value":"Bell, Centered"}`. It works the same with or without `recalculate`. A preset curve is copied into the file's `RSRCCURVDATA` the first time an assignment uses it, with a new `curv_id` and `default_flag` N; later uses, and later loads of the saved file, reuse that row. A number that is not a curve name is taken as a `curv_id` already in the file. The result gives the assignment's `curv_id`.

### Resource curves

`openxer-cli curves --input FILE [--project ID]` (MCP `openxer_curves`) lists every curve an assignment can use: the file's own curves in file order, then each OpenXER preset whose name the file does not hold. Each entry has `name`, `curv_id` (null for a preset not in the file yet), `kind` (`file`, `default` or `preset`), `in_file`, `editable`, `assignments` (how many use it), `shape` and `formula` for presets, and `points`: the 21 values `pct_usage_0` to `pct_usage_20`, the percent of the units spent in each 5% step of the duration (point 0 is the start, point k covers (k-1) x 5% to k x 5%), totalling 100.

The presets are `Even`, `Early Weighted`, `Late Weighted`, `Ramp Up`, `Ramp Down`, `Bell, Centered`, `Bell, Early Peak`, `Bell, Late Peak`, `Two Peaks` and `Steady with Ramps`, each computed from the formula `curves` reports. A file's own curve with a preset's name is the file's and hides that preset. Presets are exported only once an assignment uses one, so an XER sent elsewhere carries no curves it does not need. Curves shape how an assignment's units and costs spread over time (`applied_to_spreading` is true) on Fixed Duration and Units/Time and Fixed Duration and Units activities, and never change dates, float, stored units or leveling. `set_assignment` field `curve` reports `curve_applies`, with a `note` when the activity's duration type ignores curves. `usage` returns the spread period by period; each series has `total`, `per_period` and `unplaced`, the part of the stored quantity no period holds (outside the periods, or with nothing to spread over, such as actual units before an actual start), so `total` + `unplaced` equals the stored quantity. The exact per-period numbers follow OpenXER's reading of the documented behavior and are not yet measured against other software (`usage` reports `verified: false`).

### Writing values exactly: --recalculate false

`--recalculate false` (or `"recalculate": false` in the file, or on one `set_activity` or `set_assignment` operation) writes those values exactly into their XER columns, with no duration type, units or cost knock-on, and the project is then re-read from those columns. Use it to replay data that was written outside a scheduler. In that mode the edits also take derived fields such as `remaining_units_per_hour`, costs and raw XER column names. Scheduling still runs normally. The command line flag overrides the file's setting.

### Single edits

Each single edit command runs one operation the same way, with the same flags for input, output and mode:

```powershell
.\openxer-cli.exe set-activity --input C:\Schedules\workspace.db --project 101 --activity A1010 --field actual_finish --value "2026-04-03 17:00" --dry-run
.\openxer-cli.exe assign --input C:\Schedules\workspace.db --project 101 --activity A1015 --resource PF --output C:\Schedules\assigned.db
.\openxer-cli.exe set-assignment --input C:\Schedules\assigned.db --project 101 --activity A1015 --resource PF --field remaining_units --value 80 --output C:\Schedules\units.db
.\openxer-cli.exe calendar-day --input C:\Schedules\workspace.db --project 101 --calendar "Standard 5-day" --date 2026-07-03 --shifts nonwork --output C:\Schedules\holiday.db
.\openxer-cli.exe calendar-weekday --input C:\Schedules\workspace.db --project 101 --calendar "Standard 5-day" --weekday 6 --shifts '[["07:00","11:00"]]' --output C:\Schedules\saturdays.db
.\openxer-cli.exe set-project --input C:\Schedules\workspace.db --project 101 --field data_date --value "2026-04-06 08:00" --output C:\Schedules\dated.db
.\openxer-cli.exe add-wbs --input C:\Schedules\workspace.db --project 101 --wbs 1.4.2 --name "Tie-ins" --output C:\Schedules\wbs.db
```

Edit flags: `--input FILE (--output NEW_FILE | --dry-run) [--project ID] [--workspace-id ID] [--recalculate true|false]`. `--value` is the text as typed, so a text field keeps it exactly (`1.10`, `0042` or a 20-digit order number stay as given) and a numeric field reads its number from it; only `null` (clears) and `true` or `false` are taken as JSON. `--value ""` and `--notes ""` give empty text, and a value that starts with `--` is given inline (`--value=--draft--`). `--review-required`, `--active-only`, `--sync-overtime`, `--not-started-only` and `--work-product` mean true on their own, like `--dry-run`. `calendar-day --shifts standard` puts a date back on its weekday pattern.

## Scheduling

`schedule --options '<JSON object>'` sets the options below; `--data-date` and `--level [true|false]` are shortcuts, and giving the same option twice is refused.

| Option | Value |
| --- | --- |
| `data_date` | Project-local date or date and time |
| `must_finish` | Date, or `null` to clear the project finish constraint |
| `retained_logic`, `progress_override` | Booleans; both cannot be true |
| `lag_from_early_start`, `use_expected_finish` | Booleans |
| `level_resources` | Boolean; resource-dependent activities require true |
| `lag_calendar` | `predecessor`, `successor`, `24_hour`, `project` |
| `float_type` | `start`, `finish`, `minimum` |
| `ignore_external_relationships`, `open_ends_critical`, `use_must_finish` | Booleans |
| `multiple_float_paths`, `float_paths_use_total` | Booleans |
| `max_float_paths` | Positive integer when multiple paths are on |
| `float_paths_end_activity` | Numeric task_id of an activity in the project (another id is refused); `null` clears it |

Options left out keep the loaded settings. Scheduling uses the project's calendars, resource limits, priorities, assignments and options. Workspaces keep the resource leveling toggle; XER has no project field for it, so give `--level` again when scheduling an exported XER.

The report separates `generated` fields (null before) from `changed` fields (with `before` and `after`). Counts cover every activity; the `changes` list holds at most 200 activities, with `changes_truncated` and `change_limit`. Use `load` on the output to see the full result. Public `remaining_start` and `remaining_finish` are the exported remaining dates.

## Relationships

`relationship add|update|remove` identifies a relationship by predecessor, successor and type (`FS`, `SS`, `FF`, `SF`). `update` also takes `--new-type`. `--lag-minutes` takes signed whole working minutes from -6,000,000 to 6,000,000. Cycles, self links, duplicates and ambiguous activity codes are refused. The command keeps stored dates; run `schedule` on its output to recalculate. (The `relationship` operation of `apply` takes its lag in hours, like the other edits.)

## Import, export, Excel and PDF

```powershell
.\openxer-cli.exe import --input C:\Inbox\contractor.xer --workspace C:\Schedules\workspace.db --output C:\Schedules\merged.db --action update --config "activities=update,relationships=update+delete,calendars=keep"
.\openxer-cli.exe export --input C:\Schedules\workspace.db --projects 3,4 --version 19.12 --output C:\Schedules\both.xer
.\openxer-cli.exe export-xlsx --input C:\Schedules\workspace.db --project 101 --output C:\Schedules\101.xlsx --unit days
.\openxer-cli.exe export-xlsx --input C:\Schedules\workspace.db --project 101 --output C:\Schedules\101-tables.xlsx --layout tables
.\openxer-cli.exe import-xlsx --input C:\Schedules\workspace.db --project 101 --spreadsheet C:\Schedules\101-edited.xlsx --dry-run
.\openxer-cli.exe import-xlsx --input C:\Schedules\workspace.db --project 101 --spreadsheet C:\Schedules\101-edited.xlsx --output C:\Schedules\updated.db
.\openxer-cli.exe export-pdf --input C:\Schedules\workspace.db --project 101 --layout "Lookahead" --paper Tabloid --orientation landscape --output C:\Schedules\lookahead.pdf
```

- **import**: without `--workspace` it writes a new workspace. With `--workspace` it copies that workspace to the output and creates (`--action new`), updates (`update`), replaces (`replace`) or adds into (`add`) a project there; `--target-workspace-id` picks the row. `--config` sets per-type actions for `update`: types `activities`, `relationships`, `wbs`, `assignments`, `calendars`, `resources`, `activity_codes`, `udfs`, `notebooks`; actions `insert`, `keep`, `update`, `skip`, joined with `+`. The workspace records the XER's file name as the project's source, not its full path. The input may be a project XML file (root element `APIBusinessObjects`); it is told from XER by its contents, and `--format xer|xml` refuses a file of the other format.
- **export**: writes a new XER. Unknown tables and other projects of the source document are kept. `--version` sets the header version label (and stamps today as the export date) and keeps the rest of the plain export. `--projects` writes only the listed projects: for a workspace input each id is a workspace id, or a project id when no workspace row has that id. `--format xml`, or an output name ending in `.xml`, writes project XML instead, in the structure a project XML export of the reference format has (root element and namespaces, the order of every element's children, fractions for percents, `1`/`0` flags, hours, nil for empty fields): projects with their schedule options, WBS, activities, relationships, calendars, resources with their codes, assignments, activity codes, user defined fields, expenses, notebook topics with project, WBS and activity notes, the OBS, currencies and the financial period template, plus cost accounts, expense categories, units of measure, roles, rates and baselines where the project has them (baselines in the file as baseline projects). Every other table and column is left out of the XML. Files written this way follow that structure but have not yet been opened in the product that defines the format.
- **export-xlsx**: `--layout modern` (the default) writes typed dates, durations in the stated `--unit`, the WBS outline and Relationships, Resource Assignments and About sheets. `--layout tables` writes the TASK, TASKPRED, TASKRSRC and USERDATA sheets with XER field names in row 1. `--columns` takes column keys such as `activity_id,activity_name,original_duration,remaining_duration,start,finish,total_float` or titles. `--template` runs a saved export template by name from the workspace, or a template `.json` file; it sets columns, filter, sort and options.
- **import-xlsx**: matches rows by Activity ID. `--dry-run` returns the preview without writing: counts, every changed, new, deleted and refused row with old and new values, relationship changes and notes. Without it the same plan is applied to a new output; `--output` with `--dry-run` is refused. Deletes need an explicit `D` flag in the sheet.
- **export-pdf**: prints a layout (table and Gantt, or `--view network` for the Activity Network) to a vector PDF with its saved Page Setup. `--layout` names a saved layout (the current one when left out); `--paper` is Letter, Legal, Tabloid, Executive, A5, A4, A3, B4 (JIS), ANSI C, ANSI D or ANSI E; `--orientation` is `portrait`, `landscape` or `automatic`. Printing uses the desktop's own renderer: the command runs `OpenXER.exe` next to `openxer-cli.exe` (or the program named by `OPENXER_DESKTOP`) in a headless mode, and no window opens.

## Documents

A project's work products and documents (the WPs & Docs view: XER tables `DOCUMENT` and `TASKDOC`) are part of what an agent can read and edit. A document is named by its reference number, its title (each exact first, then ignoring case) or its document id.

```powershell
.\openxer-cli.exe documents --input C:\Schedules\workspace.db --project 101
.\openxer-cli.exe documents --input C:\Schedules\workspace.db --project 101 --activity A1010
.\openxer-cli.exe read-document --input C:\Schedules\workspace.db --project 101 --document DS-100 --limit 20000
.\openxer-cli.exe read-document --input C:\Schedules\workspace.db --project 101 --document "Tie-in drawing" --page 3 --pages 2
.\openxer-cli.exe link-document --input C:\Schedules\workspace.db --project 101 --document DS-100 --activity A1010 --output C:\Schedules\linked.db
```

`documents` (MCP `openxer_documents`) lists every document: `document_id`, `title`, `reference`, `parent`, `deliverable`, `status`, `category`, `author`, `version`, `date`, `private_location`, `public_location`, `description` (plain text), the linked `activities` (Activity ID, name, `work_product`) and the `wbs` elements it is assigned to itself (path, name). `--activity ID` or `--wbs PATH` keeps only the documents linked there. The result also lists the project file's document `statuses` and `categories`.

`read-document` (MCP `openxer_read_document`) returns the text of the file a document record points at. It is read only and never takes a free path: the file is always the document's `--location private` or `public` (by default the private location when it is set, else the public one). The location must pass the same check as Launch in the WPs & Docs view: an existing regular file named by an absolute path on a local drive (`C:\...`; on other systems an absolute path from `/`). Relative locations are refused, as Launch refuses them, and so are network (UNC) paths, device paths (elsewhere, files under `/dev`, `/proc` and `/sys`), web and other addresses, alternate data streams (Windows), and locations that resolve through a link to something else. Programs and scripts (`.exe .bat .cmd .ps1 .js .vbs .msi .com .scr`) are never read.

A schedule received from someone else can name any file on this computer in its document rows, and an agent reads without a person clicking Launch. So `read-document` reads only files under the folder of the `--input` file (after links are resolved). To allow other folders, set the environment variable `OPENXER_DOCUMENT_ROOTS` for the `openxer-cli` or MCP server process to those folders, separated by `;` (for example `C:\Projects\Docs;D:\Drawings`; on other systems by `:`, as in `PATH`). A file elsewhere is refused with `location_refused`, and the message says how to allow its folder.

| Type | Returned as | Paging |
| --- | --- | --- |
| `txt`, `md`, `csv`, `json`, `xml`, `log` | `text` (UTF-8, UTF-16 with a byte order mark, else Windows-1252) | `--offset` and `--limit` characters (default 20000, at most 200000); `next_offset` continues |
| `docx` | `text`, the body paragraphs one per line | as text |
| `pdf` | `pages`: `[{page, text}]`, each page extracted on its own | `--page` (from 1) and `--pages` (default 10, at most 100), the characters capped by `--limit`; `next_page` continues. A page cut by `--limit` carries `truncated: true` and `page_characters`, the result has `truncated: true`, and `next_page` names that page again: read it with a larger `--limit` |

PDF text comes from the embedded text; a scanned drawing without a text layer returns empty pages. Encrypted PDFs are not read. Files over 64 MB are not read at all. Error codes: `document_not_found`, `no_location`, `location_refused`, `refused_type`, `unsupported_type`, `file_too_large`, `read_failed` and `invalid_arguments` (paging that does not fit the file).

Edits use the same operations as the WPs & Docs view (`apply` or the single edits, MCP tools of the same names):

- `{"op":"add_document","title":"Pump datasheet","reference":"DS-100","parent":"DS-000","fields":{"version":"B","public_location":"C:\\Docs\\ds-100.pdf"}}` adds a document after its last sibling, under `parent` or at the top level. The result gives its `document_id`.
- `{"op":"set_document","document":"DS-100","field":"status","value":"Approved"}` sets `title`, `reference`, `deliverable` (yes or no), `status` and `category` (by name, from the file's `DOCSTAT` and `DOCCATG` dictionaries), `author`, `version`, `date` (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`), `private_location`, `public_location` or `description` (plain text). `null` clears; a title cannot be cleared.
- `{"op":"delete_document","document":"DS-100"}` deletes the document with the documents under it and all their links.
- `{"op":"link_document","document":"DS-100","activity":"A1010","work_product":true}` links it to an activity, or with `"wbs":"1.2"` to a WBS element itself; `{"op":"unlink_document",...}` removes that link.

Dry runs report the changes under `documents` and `document_links` in the diff.

## Expenses, issues, risks and thresholds

The Project menu's registers (XER tables `PROJCOST`, `PROJISSU`, `PROJRISK` or `RISK` with `TASKRISK`, and `PROJTHRS` with its parameters in `THRSPARM`) are read with `records` and edited with the register operations, through the same functions as the Expenses, Issues, Risks and Thresholds views and the Activity Details tabs.

```powershell
.\openxer-cli.exe records --input C:\Schedules\workspace.db --project 101 --kind issues
.\openxer-cli.exe add-expense --input C:\Schedules\workspace.db --project 101 --activity A1010 --name "Crane rental" --fields '{"budgeted_units":4,"price_per_unit":1250}' --output C:\Schedules\crane.db
.\openxer-cli.exe set-issue --input C:\Schedules\crane.db --project 101 --issue "Late steel" --field status --value Closed --output C:\Schedules\closed.db
.\openxer-cli.exe monitor-thresholds --input C:\Schedules\closed.db --project 101 --dry-run true
```

`records --kind expenses|issues|risks|thresholds [--activity ID]` (MCP `openxer_records`) lists each record's `id` and every field below with the value as the edits take it; risks list their `activities`, thresholds come with the `parameters` list. A record is named by its id, or by its name when that names one record (a risk also by its Risk ID). Choices take their code or label, dictionary fields a name from the file's dictionary (or its id), `null` clears.

| Record | Fields |
| --- | --- |
| expense | `name`, `activity` (moves the item), `category` (`COSTTYPE`), `cost_account` (`ACCOUNT`), `accrual` (`Start of Activity`, `End of Activity`, `Uniform`), `unit_of_measure`, `vendor`, `po_number`, `auto_compute_actuals`, `budgeted_units`, `price_per_unit`, `budgeted_cost`, `actual_cost`, `remaining_cost`, `description` |
| issue | `name`, `responsible_manager` (`OBS`), `priority` (`Top` to `Lowest`), `status` (`Open`, `On Hold`, `Closed`), `identified_on`, `identified_by`, `resolution_date`, `activity` or `wbs` (setting one clears the other), `resource` (short name), `notes`; read only: `threshold_parameter`, `actual_value`, `lower_threshold`, `upper_threshold` |
| risk | `risk_id`, `name`, `category` (`RISKTYPE`), `type` (`Threat`, `Opportunity`), `status` (`Proposed`, `Open`, `Active`, `Rejected`, `Impacted`, `Managed`), `owner` and `identified_by` (resource short names), `identified_on`, `pre_response_probability`, `pre_response_schedule`, `pre_response_cost`, `post_response_probability`, `post_response_schedule`, `post_response_cost`, `response_type`, `response_description`, `description`, `cause`, `effect`, `notes` |
| threshold | `parameter` (name, short name such as `Total Float`, or measured field), `lower_threshold`, `upper_threshold`, `detail_to_monitor` (`Activities`, `WBS`), `wbs`, `responsible_manager`, `priority`, `status` (`Enabled`, `Disabled`), `from_date`, `to_date` |

The edits follow the views: an expense item's budgeted units times its price is its budgeted cost, and while it has no actual cost its budget is all remaining. New issues are open and new risks proposed threats with the next free `RISK-n`, both identified today; a new threshold is an enabled total float threshold with a lower bound of 0 days until its fields say otherwise. Deleting an issue removes its history, deleting a risk its activity assignments; issues a deleted threshold raised stay.

`monitor_thresholds` (CLI `monitor-thresholds`, MCP `openxer_monitor_thresholds`) runs Tools > Monitor Thresholds: every enabled threshold is evaluated over the activities or WBS elements of its WBS branch inside its window (or the `from` and `to` given), and an issue is raised for each value below the lower or above the upper bound, or the open issue it raised before is updated. Float and the start and finish variances are in days of the activity's calendar, the variances against the project baseline; the cost and earned value parameters (AV, CV, SV, VAC, CPI, SPI, CVI, SVI, Cost % of Budget, Duration % of Original) are computed as the earned value columns compute them. For a workspace input the project baseline and the primary baseline are the ones assigned in the app, read from the workspace; an XER input, or a project without them, is its own baseline. The result gives `thresholds`, `issues_raised`, `issues_updated`, `skipped` and the `baselines` used.

## Progress, steps and the Tools edits

The weekly update runs through the same rules as the Tools menu:

```powershell
.\openxer-cli.exe update-progress --input C:\Schedules\workspace.db --project 101 --data-date "2026-04-13 08:00" --output C:\Schedules\progressed.db
.\openxer-cli.exe schedule --input C:\Schedules\progressed.db --project 101 --output C:\Schedules\week-15.db
.\openxer-cli.exe set-step --input C:\Schedules\week-15.db --project 101 --activity A1010 --step "Pour" --field complete --value true --dry-run true
```

- `update_progress` records progress as if every activity (or only `activities`) ran as scheduled up to the new data date: activities that should have started start on their scheduled start (never before the old data date), those that should have finished finish, and the remaining duration runs to the scheduled finish (`remaining` `always`), or for fixed units types uses up the elapsed working time (`duration_type`). LOE and WBS summary activities are left alone. The data date moves; schedule afterwards. Any data date is accepted: one equal to or earlier than the current data date only sets the data date, with no progress added and none removed (`apply_actuals` also stamps an earlier date as the apply actuals date).
  Expenses set to compute actuals (uniform accrual) on the activities it progresses get the budget's share of the worked time as actual cost.
- `apply_actuals` follows the Auto Compute Actuals flags: an activity with the flag, or whose assignments all have resources with it, progresses as above together with all its assignments; flagged resources on only some assignments give just those assignments actuals and leave the activity's dates and duration. An assignment's actual units grow by its budgeted units/time times the working time from its remaining start to the new data date (at most its remaining units), costs follow at its price, and actual this period grows with them when the project links the two. Flagged expenses of started activities accrue as above. A project already at the new data date is left alone. It stamps the project's apply actuals date.
- `recalculate_costs` reprices assignments from their resources' prices and reports `updated`, `unchanged` and the `skipped` assignments with the reason.
- `top_down_estimate` spreads labor units over the receiving activities by the WBS estimation weights, as Tools > Top Down Estimation does.
- Steps keep their order (`seq_num` 1, 2, ...). Setting `percent_complete` to 100 completes a step; `complete` false sets it back to 0. When the project's steps drive percent complete, the activity's physical percent follows every step change, as in the Steps tab. Step templates are the project file's `PROCGROUP` and `PROCITEM` rows.
- `shift_wbs` and `rename_wbs_code` are the WBS view's Shift buttons and code field; the calendar edits are the Calendars dialog's Add (copy), rename, Set As Default and convert.

## Global Change

`global_change` (CLI `global-change`, MCP `openxer_global_change`) runs Tools > Global Change on every activity of the project, with the same definitions, evaluation and writes as the app's Global Change dialog and report (`openxer_xer::edit::global_change`). Name a definition saved in a workspace input with `name`, or give one as `rule`:

```powershell
.\openxer-cli.exe global-change --input C:\Schedules\workspace.db --project 101 --name "Increase Durations" --dry-run
.\openxer-cli.exe global-change --input C:\Schedules\plan.xer --rule '{"match":"all","conditions":[{"column":"WBS","op":"under","value":"PLANT.2"}],"then":[{"column":"Original Duration","value":"Original Duration","operator":"x","value2":"1.5"}]}' --output C:\Schedules\longer.xer
```

- `conditions` are the If rows: `column` (a field id such as `activity_status` or `udf:Crew`, or its title), `op` (`eq`, `ne`, `contains`, `not_contains`, `starts_with`, `lt`, `le`, `gt`, `ge`, `within`, `not_within`, or its caption such as `is greater than`), `value` and, for the ranges, `high`. `under` ("is under") holds for the WBS element or activity code value named and every one below it; it is offered for WBS and activity code fields only. `match` `all` (the default) needs every row, `any` one of them.
- `then` rows set a field on the activities that meet the If rows and `otherwise` rows on the others: `column`, `value` (a literal, the title of another field, or text in double quotes) and optionally `operator` (`+`, `-`, `x`, `/`, `&` to join text) with `value2`. Any editable field can be set, user defined fields and activity codes included; a value goes through the same rules as a value typed into the activity table, so a locked field is refused for that activity.
- The result's `rows` are the lines of the Global Change Report: `project`, `wbs`, `activity`, `field`, `field_id`, `old`, `new` and `exception` (why that activity's value cannot change). Durations are hours (`40h`), dates `YYYY-MM-DD HH:MM`. With `--dry-run` nothing is written. Otherwise every line without an exception is written, and `changed` counts them, all in one operation of the batch.
- The app applies a global change only to the activities its Activities filter shows; outside the app there is no window filter, so put the activities you mean in the If rows.
## S-curve and earned hours

`export-scurve --input FILE --output NEW.xlsx [--measure auto|labor|cost|duration] [--period day|week|month|financial] [--breakdown total|resource|wbs1|wbs2] [--late] [--week-start sunday|monday]` (MCP `openxer_export_scurve`) writes the workbook of Tools > Reports > S-Curve, from the same figures (`xer/src/scurve.rs`) and writer (`interchange/src/scurve_xlsx.rs`):

- **S-Curve**: one row per period (start, end) with budgeted, actual and remaining values, their running sums and percents of the budget as formulas, the earned value to the data date in the data date's row, a total row and a native chart (period budgeted and actual as columns, cumulative budgeted, actual and actual plus remaining as lines on the secondary axis, earned as a marker).
- **By Resource - Budgeted**, **- Actual**, **- Remaining** (or by WBS level 1 or 2): one row per resource or WBS element, one column per period, total row and column as formulas. Not written with `--breakdown total`.
- **Info**: project, data date, measure and what was found, period, breakdown, where the budget comes from, generation time and warnings.

`--measure auto` (the default) takes labor units when labor assignments carry budgeted units, else cost (assignments and expenses), else duration: each activity weighs its planned (original) duration in working days on its calendar, spread over its planned working days, actual is that weight times the activity % complete. Budgeted comes from the assigned project baseline (or the primary baseline when the project chooses it), spread over the baseline's own planned dates; else from the current plan. Actual runs to the data date and remaining from it on the early dates; `--late` adds actual plus remaining on the late dates. Weeks start on Sunday by default. The spreads are the usage views' (resource curves included), so the per-period totals equal the Activity Usage Profile's for the same periods.

`export-earned-hours --input FILE --output NEW.xlsx [--breakdown resource|wbs|activity]` (MCP `openxer_export_earned_hours`) writes cumulative earned labor hours to the data date: **Earned to date** (one row per resource, WBS element or activity: `Budgeted Units`, `Earned Units`, `Actual Units`, `Remaining Units`, `At Completion Units`, `% Earned`, a total row) and **By activity** (one row per labor assignment, sorted by Activity ID then Resource ID). Earned units are the baseline's budgeted units of the assignment (the current budget without a baseline) times the activity's performance % complete, as `earned-value` computes it. Rows and columns keep a stable order, so the period's earned hours are one export's Earned Units minus the last update's. A project without labor hours fails with `no_labor_units` and writes nothing.

## Earned value

`earned-value --input FILE [--basis cost|labor_units] [--activity ID]` (MCP `openxer_earned_value`) returns, for each activity and for the project, `budget_at_completion`, `planned_value`, `earned_value`, `actual_cost`, `schedule_variance`, `cost_variance`, `schedule_performance_index`, `cost_performance_index` (null when the divisor is zero), `estimate_to_complete`, `estimate_at_completion` and `variance_at_completion`, with each activity's `performance_percent_complete` and `schedule_percent_complete`. They are the values of the activity table's earned value columns: the WBS elements' earned value techniques apply (`set_wbs_earned_value`), against the baselines described for Monitor Thresholds and the workspace's earned value date choice. Read only.

## Schedule analysis

Read-only commands that return the numbers a schedule review needs, so an agent reports them instead of working them out. Each takes `--input FILE` and `--project ID` or `--workspace-id ID`; dates and float are those last scheduled and stored in the file. Lists hold 25 items and count the rest in `more`.

| Command | MCP | Returns |
| --- | --- | --- |
| `check-schedule [--long-lag-hours N] [--large-float-hours N] [--large-duration-hours N]` | `openxer_check_schedule` | Tools > Check Schedule's 15 checks (the same code as the app): `count`, `out_of`, `value`, `target`, `passed`, `note` and the activities or ties `found`; the late activities and baseline execution index checks use the workspace project's project baseline. `logic_warnings`: without predecessors, without successors, constrained, actuals after the data date, milestones with odd ties, finish milestones fed from another calendar, out-of-sequence ties. |
| `paths [--activity ID] [--count 1-10]` | `openxer_paths` | The scheduler's multiple float paths ending at the activity (the open activity finishing last when omitted): per path its `lowest_total_float_hours`, `behind_path_1_hours`, calendars, what holds its first activity and its activities in order; `merge_points`; `critical_counts` (on the longest path, total float zero or less, negative). |
| `compare (--reference FILE [--reference-project ID] [--reference-workspace-id ID] \| --baseline project\|primary)` | `openxer_compare` | The project against an earlier file or a stored baseline of the workspace project: `project_finish` then, now and moved, a `summary` of counts, and sections (each `items`, `more`, `count`) for milestones moved, biggest finish moves, activities added and deleted, ties added, removed and changed (type or lag), original durations changed, remaining duration up on work in progress, constraints and calendars changed, renamed, and progress (started, finished, actual dates rewritten, status gone back). Activities match by Activity ID and ties by predecessor and successor through the same core as Tools > Schedule Comparison (`xer/src/compare.rs`); moves are calendar days. |
| `lookahead [--weeks 1-26] [--from DATE]` | `openxer_lookahead` | Open work in progress or due in the window from the data date, by WBS: each activity with its `state` (in progress, ready, waiting), `waiting_on` (open predecessors with status and expected finish) and resources. At most 300 activities are listed. |
| `milestones` | `openxer_milestones` | Every milestone in date order with date, float, constraints, the project baseline date and move, and `driven_by` (the driving ties into it). |
| `schedule-stats [--update-period-days N]` | `openxer_schedule_stats` | Activities by type and status; every calendar with its week, hours per working day and exception dates; original durations in working days (median, 90th percentile, spread, longest, zero-duration tasks, longer than two update periods); repeated and one-word names; open work without resources or expenses; activities by WBS level and empty WBS elements. |
| `resource-loading [--interval day\|week\|month]` | `openxer_resource_loading` | Per assigned resource: actual plus remaining units per period (the usage spread), the peak, the limit in units per hour at the data date and the periods over it (units per working hour of the project's default calendar). |
| `wbs-summary [--wbs-level 1-10]` | `openxer_wbs_summary` | The schedule rolled up by WBS element down to the level: activities, completed, start, finish, lowest total float, baseline finish and move, and its milestones. |

## Skills for AI agents

A skill is a Markdown file of instructions that teaches any connected agent one way to analyze (`kind: check`, read-only) or build and change (`kind: workflow`) a schedule with these tools. OpenXER ships a library of them, built from `assets/skills/` into the executable. The user's own are `.md` files in `<data folder>\skills\` (the folder that holds the default workspace; `OPENXER_DATA_DIR` moves it); the app's **AI Agents > Skills** dialog edits them. A user file whose id equals a built-in's replaces it (customized); deleting the file brings the built-in back. A file that does not parse is left out and reported with its line and problem; the others still load. The folder is read on every request, so new and edited skills reach a running MCP server at once.

```text
---
id: crew-check                # the file name without .md: a-z, 0-9, single hyphens
name: Crew check              # at most 40 characters
description: One line, at most 160 characters.
request: Run the crew check.  # the sentence a user pastes to the agent
use_when: The user asks about crews.
kind: check                   # check or workflow
order: 50                     # optional, 0 to 999
examples:                     # optional
- Are the crews overloaded?
tools:                        # the OpenXER tools the steps run
- openxer_resource_loading
---
The instructions, in Markdown.
```

`cites:` (a list) is accepted for the author's own notes; it is never sent to an agent. A tool a skill names that the server does not have is a warning.

| Command | MCP | Does |
| --- | --- | --- |
| `skills list` | `openxer_skills` | Every skill: `id`, `name`, `description`, `use_when`, `request`, `kind`, `source` (`built_in`, `yours`, `customized`), plus the files left out (`problems`) and the folder. |
| `skills show ID` | `openxer_skill` | One skill with its `instructions`, `examples`, `tools` and the file it came from. |
| `skills validate [FILE]` | | One file (error code `invalid_skill` with the line and problem), or every file in the folder. |
| `skills new ID` | | Writes the self-explaining template to `<folder>\ID.md`; an existing file is refused. |
| `skills path` | | The skills folder and the data folder. |

MCP also offers every skill as a prompt (`prompts/list`, `prompts/get`, named by id), for clients with a prompt menu. The server's `instructions` and the skill tools tell the agent to look for a matching skill before analyzing or building a schedule, and never to compute schedule metrics itself.

## Project folders (EPS) and rollups

The EPS is the folder tree that files a workspace's projects. XER files do not carry it; it lives in the workspace database, and these commands use the same code as the desktop app (Enterprise > Project Folders, the Projects view and the New Project wizard), so the rules are the same.

```powershell
.\openxer-cli.exe eps list --input C:\Schedules\workspace.db
.\openxer-cli.exe eps add-folder --input C:\Schedules\workspace.db --output C:\Schedules\w1.db --name "East Region" --eps-id EAST --parent WORKSPACE
.\openxer-cli.exe eps file-project --input C:\Schedules\w1.db --output C:\Schedules\w2.db --workspace-id 3 --folder EAST
.\openxer-cli.exe eps create-project --input C:\Schedules\w2.db --output C:\Schedules\w3.db --folder EAST --project-id TA-27 --name "Turnaround 2027" --planned-start 2027-01-04 --must-finish 2027-03-30
.\openxer-cli.exe eps rename-folder --input C:\Schedules\w3.db --output C:\Schedules\w4.db --folder EAST --name "Eastern Region" --new-eps-id EASTERN
.\openxer-cli.exe eps move-folder --input C:\Schedules\w4.db --output C:\Schedules\w5.db --folder EASTERN
.\openxer-cli.exe eps delete-folder --input C:\Schedules\w5.db --dry-run --folder EASTERN
.\openxer-cli.exe eps delete-folder --input C:\Schedules\w5.db --output C:\Schedules\w6.db --folder EASTERN --delete-projects
```

- On a workspace with a shared dictionary (workspace schema 3) `eps list` adds the dictionary's calendars, resources and codes to each project as the app does; `create-project` takes the responsible managers from the dictionary and stores the new project's calendar there, as the app's wizard does.
- **create-project** gives the new project an internal project number (the XER `proj_id`, returned as `project_number`) that no workspace project or baseline uses, as File > New does; relationships between projects name the other project by this number.
- **eps list** returns `folders` (each with `eps_id`, `name`, `parent`, `depth`, `path`, the workspace ids of the `projects` filed directly in it and a `rollup` over every project under it, child folders included), `projects` (each with `workspace_id`, `project_id`, `name`, `status`, `folder` and its `rollup`) and `rules`. Values are read from the stored projects, so closed projects are always current.
- A **rollup** has `start` and `finish` (the earliest start and latest finish of the activities, actual else early), `late_start`, `late_finish`, `actual_start`, `actual_finish` (only when every activity is completed), `data_date` (a folder shows the latest of its projects), `must_finish_by` (a folder: the latest), `baseline_finish` and `finish_variance_hours` (baseline finish minus finish in working hours; the project baseline, else a stored Primary baseline), `total_float_hours` (the lowest of the unfinished activities; a folder: the lowest of its projects), `schedule_percent_complete`, `labor_units_hours` and `costs` (each `budgeted`, `actual`, `remaining`; labor units count labor resources, costs every assignment plus the expenses; they add up), the counts `activities`, `not_started`, `in_progress`, `completed` and `projects`, and the `working_time_calendar`.
- **schedule_percent_complete** is duration based: (planned span minus remaining span) divided by planned span, times 100. The planned span runs from the earliest start (the earliest actual start once work has begun) to the latest finish; the remaining span from the earliest remaining start of an unfinished activity (the data date when none is stored) to the latest remaining finish. Every activity completed is 100. Spans are measured in working time of the project's default calendar; a folder uses its projects' shared default calendar, else the standard calendar new projects start with (Monday to Friday, 08:00 to 12:00 and 13:00 to 17:00).
- The edits name folders by EPS ID (letter case ignored) and write a copy to `--output` (or preview with `--dry-run`). `add-folder` adds the folder last under `--parent` (the top level when left out); names must be unique among siblings and EPS IDs in the whole tree. `move-folder` without `--parent` moves the folder to the top level; a folder cannot move under itself. `delete-folder` deletes the folder, the folders under it and every project filed in any of them, with those projects' baselines and baseline choices, as the app's Delete does after its one question. A folder that holds projects is refused (error code `refused`, with the count) unless `--delete-projects` is given; `--dry-run` lists what would go (`deleted` folders, `projects_deleted`, `baselines_deleted` and `items`, folders plus projects) without it. The last top-level folder cannot be deleted. The input workspace is never changed, so it stays the way back. `file-project` puts a workspace project in a folder. `create-project` makes an empty project with the New Project wizard's rules: a unique Project ID of up to 20 characters, a name of up to 100 (the ID when left out), a Planned Start (08:00 when no time is given; it is also the data date), an optional Must Finish By (17:00 when no time is given; not before the start), an optional `--manager` (a responsible manager's name or OBS id from the project last open in the app) and `--rate-type` (`COST_PER_QTY` to `COST_PER_QTY5` or a name such as `Price / Unit2`; the default rate type of the project's new assignments).

## Baselines

A baseline is a frozen copy of a project kept in the workspace beside it; it is not a project and never appears in the project lists. These commands use the app's own rules (Project > Maintain Baselines, Assign Baselines and Revert Project to Baseline; `automation/src/baseline_ops.rs`).

```powershell
.\openxer-cli.exe baseline list --input C:\Schedules\workspace.db --workspace-id 3
.\openxer-cli.exe baseline add --input C:\Schedules\workspace.db --workspace-id 3 --output C:\Schedules\b1.db
.\openxer-cli.exe baseline set --input C:\Schedules\b1.db --workspace-id 3 --output C:\Schedules\b2.db --baseline "TA-27 - B1" --name "Contract plan" --type "Initial Planning Baseline"
.\openxer-cli.exe baseline assign --input C:\Schedules\b2.db --workspace-id 3 --output C:\Schedules\b3.db --project-baseline "Contract plan" --primary "Contract plan"
```

- **baseline list** returns `projects`, each with `workspace_id`, `project_id`, `name`, its `baselines` (each `id`, `baseline_id`, `name`, `type`, `data_date`, `date_added`, `last_update_date` and `assigned`: the roles `project`, `primary`, `secondary`, `tertiary` it holds), `assigned` (the Assign Baselines choices: `current`, `none` or a baseline) and the `baseline_types`.
- The edits take `--workspace-id` (the project whose baselines change) and write a copy to `--output` (or preview with `--dry-run`). A baseline (REF) is named by its `id`, its Baseline ID or its name. A refused edit fails with the error code `refused` and writes nothing.
- **add** saves a copy of the project as Baseline ID "<project ID> - B<n>" and name "<project name> - B<n>", n the lowest number no baseline of the project uses; its last update date is the time it was taken. **convert --project N** makes the workspace project N a baseline that keeps its own ID and name, with a blank last update date; the project itself is refused, and so is a project with baselines of its own. **copy** makes another baseline of the project with the next number, the source's data, data date, type and last update date.
- **delete** and **restore** refuse a baseline assigned as the project baseline or as a primary, secondary or tertiary baseline. **restore** makes the baseline a separate project with the same ID and name and an internal project number of its own, filed in the owner's EPS folder; a Project ID already in use is refused.
- **revert** (Revert Project to Baseline) replaces the project's data with the baseline's; the project keeps its ID, name, project baseline and other baselines, and the baseline is not changed.
- **update --baseline REF** runs Maintain Baselines > Update: the baseline takes the project's data by the Update Baseline options and its last update date is set; an assigned baseline may be updated. `--options` is a JSON object of the options by name (omitted ones take the dialog's defaults): General `project_details` (on), `project_udfs`, `risks_issues`, `risk_udfs`, `issue_udfs`, `documents`, `document_udfs`, `wbs_udfs`, `log_to_file` (on) with `log_path` (a log is written only when a path is given), `ignore_last_update` and `run_optimized` (accepted, no effect: every update compares in full); Activities `add_new`, `delete_missing`, `update_existing` (on); Activity `general` (on), `relationships`, `constraints`, `dates`, `wbs` (on), `risks`, `activity_udfs`, `codes` (on), `notebooks` (on), `steps` (on), `step_udfs`, `expenses`, `expense_udfs`, `budget_no_assignments`, `actual_no_assignments`; Resource Assignment `update_assignments`, `update_assignment_budget`, `update_assignment_actual`, `add_assignments` (on), `add_assignment_budget` (on), `add_assignment_actual` (on), `assignment_udfs`, `assignment_codes`. A child option acts only with its parent on. `--activities ID,ID` limits the activities added or updated, as the filters do in the app (the app's `use_filter`, `filters` and `filter_match_all` are kept with the options but evaluated only there; the result echoes the `options` used). Activities are matched by GUID, then by Activity ID, so a renumbered activity is updated rather than deleted and added. The result's `preview` lists `project`, `wbs` and `activities` changes, each `object`, `kind` (`Added`, `Deleted`, `Updated`) and `fields` (`field`, `project`, `baseline`), with the counts; `--dry-run true` returns it and writes nothing.
- **set** changes `--baseline-id`, `--name` and `--type` (a baseline type name, or `none`). **assign** sets `--project-baseline` (`current` or a baseline) and `--primary`, `--secondary`, `--tertiary` (`current`, `none` or a baseline); a slot left out keeps its choice.

## JSON contract
Every command prints one JSON object on stdout:

```json
{"schema_version":"1.0","ok":true,"result":{}}
```

`apply` (MCP `openxer_apply`) runs an `openxer-changes/1` batch; `openxer-cli.exe --help` lists every operation. Activity codes can be built from nothing: `{"op":"add_code_type","code_type":"Area","scope":"project","max_length":10}` (scope `project`, `eps` or `global`, default `project`; `max_length` 1 to 60, default 60), then `add_code_value` and `set_activity_code`.

Project funding names its funding source by name (the `FUNDSRC` dictionary, Enterprise > Funding Sources in the app); a project lists each source once. `{"op":"add_project_funding","source":"Capital","amount":250000,"share":40}` adds it (amount and share default to 0; share is a percentage from 0 to 100), `{"op":"set_project_funding","source":"Capital","field":"amount","value":300000}` changes `amount`, `share` or `source` (another source's name), and `{"op":"delete_project_funding","source":"Capital"}` removes it. The rows are `PROJFUND` and export with the project.

Resources are named by short name (Resource ID). `{"op":"merge_resources","from":["PIPE-2","PIPE.1"],"into":"PIPE"}` merges duplicates (as left by importing XERs from several sources) into one resource: assignments in every project of the file and the activities' primary resource move to `into`; rate rows, role links, resource codes and resource UDF values move unless `into` already has a rate from that date, that role, a value of that code or of that field (then `into` keeps its own); child resources move under `into`, which takes a source calendar only when it has none, and any other table naming the resource is re-pointed. Where `into` and a source are assigned to the same activity the assignments become one: the target's own row stays, budgeted, actual, remaining and this-period units and costs and units per time are summed, start dates take the earliest and finish dates the latest (no actual finish while a combined started assignment is open), the kept row's role, rate type, price and curve stay, and its period rows, assignment codes and assignment UDF values take over the others'. Prices are not recalculated. The resources must be of one type. The result lists what moved (`assignments_moved`, `combined`, `rates`, `roles`, `codes`, `udf_values`, `child_resources`, `calendar_taken`, `other_rows`) and a readable `summary`; run it with `--dry-run` (MCP `dry_run`) to preview. `{"op":"delete_resource","resource":"SPARE"}` deletes a resource with no assignments and no child resources.

`{"op":"set_wbs_earned_value","wbs":"RB.3","performance":"custom","custom_percent":35,"estimate_to_complete":"pf","performance_factor":1.25}` sets a WBS element's earned value techniques (`wbs` "" is the project node, which elements without their own settings follow). `performance` is `activity_percent`, `wbs_milestones`, `0/100`, `50/50` or `custom` (with `custom_percent` 0 to 100); `estimate_to_complete` is `remaining`, `pf_1`, `pf_cpi`, `pf_cpi_spi` or `pf` (with `performance_factor` above 0). XER codes (`EC_50_50`, `EE_PF_cpi`) are accepted too. They are the `PROJWBS` fields `ev_compute_type`, `ev_user_pct`, `ev_etc_compute_type` and `ev_etc_user_value`, which the earned value columns read and which export with the WBS.

Errors are `{"schema_version":"1.0","ok":false,"error":{"code":"output_exists","message":"..."}}`. The exit status is 0 for success, 2 for invalid arguments or requests (error code `invalid_arguments` or `invalid_changes`, whether the command line or the request was wrong) and 1 for any other failed command. `--help` and `--version` print plain text. Paths are local to the process; the Windows executable expects Windows paths.

Dates in results are project-local `YYYY-MM-DD HH:MM` strings or `null`; there is no time zone conversion. In `load`, `schedule` and `relationship` requests and results, durations, lags and floats are whole **working minutes**; the edit operations take hours.

Error codes: `invalid_arguments`, `not_found`, `refused`, `invalid_changes`, `invalid_source`, `invalid_relationship`, `invalid_spreadsheet`, `project_not_found`, `ambiguous_project`, `relationship_not_found`, `ambiguous_relationship`, `template_not_found`, the document read codes above, `unsupported_schedule`, `schedule_rejected`, `operation_failed`, `milestone_assignment` (an edit would give a milestone a resource or role assignment), `import_rejected`, `export_rejected`, `export_failed`, `output_exists`, `io_error`, `no_baseline` (compare: no stored baseline assigned), `invalid_skill` and `write_failed` (skills). Unknown request fields are refused. A failed edit or schedule leaves the project unchanged.

## MCP

Run the same executable with `mcp`. Only newline-delimited JSON-RPC goes to stdout; diagnostics go to stderr. The process exits when stdin closes. Example client configuration:

```json
{
  "mcpServers": {
    "openxer": {
      "command": "C:\\Tools\\OpenXER\\openxer-cli.exe",
      "args": ["mcp"]
    }
  }
}
```

The release package ships this block as `openxer.mcp.json` with `<full path to openxer-cli.exe>` in place of the path; put the full path of your copy there, or let `openxer-cli connect` (below) write the entry.

Tools:

| Tool | Writes | Same as |
| --- | --- | --- |
| `openxer_find_schedules` | no | `find-schedules` |
| `openxer_inspect` | no | `inspect` |
| `openxer_load` | no | `load` |
| `openxer_check_schedule` | no | `check-schedule` |
| `openxer_playbook` | no | `playbook` |
| `openxer_curves` | no | `curves` |
| `openxer_usage` | no | `usage` |
| `openxer_import` | yes | `import` |
| `openxer_schedule` | yes | `schedule` |
| `openxer_relationship` | yes | `relationship` |
| `openxer_apply` | yes | `apply` (`changes` holds the document itself, or `changes_file` a path) |
| `openxer_set_activity` | yes | `set-activity` |
| `openxer_add_activity` | yes | `add-activity` |
| `openxer_delete_activity` | yes | `delete-activity` |
| `openxer_dissolve_activity` | yes | `dissolve-activity` |
| `openxer_assign` | yes | `assign` |
| `openxer_unassign` | yes | `unassign` |
| `openxer_set_assignment` | yes | `set-assignment` |
| `openxer_calendar_day` | yes | `calendar-day` |
| `openxer_calendar_weekday` | yes | `calendar-weekday` |
| `openxer_set_project` | yes | `set-project` |
| `openxer_add_wbs` | yes | `add-wbs` |
| `openxer_add_document` | yes | `add-document` |
| `openxer_set_document` | yes | `set-document` |
| `openxer_delete_document` | yes | `delete-document` |
| `openxer_link_document` | yes | `link-document` |
| `openxer_unlink_document` | yes | `unlink-document` |
| `openxer_add_expense`, `openxer_set_expense`, `openxer_delete_expense` | yes | `add-expense`, `set-expense`, `delete-expense` |
| `openxer_add_issue`, `openxer_set_issue`, `openxer_delete_issue` | yes | `add-issue`, `set-issue`, `delete-issue` |
| `openxer_add_risk`, `openxer_set_risk`, `openxer_delete_risk`, `openxer_assign_risk`, `openxer_unassign_risk` | yes | `add-risk`, `set-risk`, `delete-risk`, `assign-risk`, `unassign-risk` |
| `openxer_add_threshold`, `openxer_set_threshold`, `openxer_delete_threshold` | yes | `add-threshold`, `set-threshold`, `delete-threshold` |
| `openxer_monitor_thresholds` | yes | `monitor-thresholds` |
| `openxer_update_progress`, `openxer_apply_actuals`, `openxer_recalculate_costs`, `openxer_top_down_estimate` | yes | `update-progress`, `apply-actuals`, `recalculate-costs`, `top-down-estimate` |
| `openxer_add_step`, `openxer_set_step`, `openxer_delete_step`, `openxer_move_step`, `openxer_add_steps_from_template`, `openxer_create_step_template` | yes | `add-step`, `set-step`, `delete-step`, `move-step`, `add-steps-from-template`, `create-step-template` |
| `openxer_set_feedback` | yes | `set-feedback` |
| `openxer_shift_wbs`, `openxer_rename_wbs_code` | yes | `shift-wbs`, `rename-wbs-code` |
| `openxer_global_change` | yes | `global-change` |
| `openxer_copy_calendar`, `openxer_rename_calendar`, `openxer_set_default_calendar`, `openxer_convert_calendar` | yes | `copy-calendar`, `rename-calendar`, `set-default-calendar`, `convert-calendar` |
| `openxer_records` | no | `records` |
| `openxer_earned_value` | no | `earned-value` |
| `openxer_documents` | no | `documents` |
| `openxer_read_document` | no | `read-document` |
| `openxer_export` | yes | `export` |
| `openxer_export_xlsx` | yes | `export-xlsx` |
| `openxer_import_xlsx` | yes | `import-xlsx` |
| `openxer_export_pdf` | yes | `export-pdf` |
| `openxer_export_scurve` | yes | `export-scurve` |
| `openxer_export_earned_hours` | yes | `export-earned-hours` |
| `openxer_eps_list` | no | `eps list` |
| `openxer_eps_edit` | yes | the `eps` edits (`op` is `add_folder`, `rename_folder`, `move_folder`, `delete_folder`, `file_project` or `create_project`, with the edit's fields beside it; `delete_folder` takes `delete_projects`) |
| `openxer_workspace_migrate` | yes | `workspace migrate` |
| `openxer_workspace_dictionary` | no | `workspace dictionary` |
| `openxer_workspace_dictionary_usage` | no | `workspace dictionary-usage` |
| `openxer_workspace_preference` | with `value` | `workspace preference` |
| `openxer_baseline_list` | no | `baseline list` |
| `openxer_baseline_update` | yes | `baseline update` (`workspace_id`, `baseline`, `options`, `activities`, `dry_run`) |
| `openxer_baseline_edit` | yes | the `baseline` edits (`workspace_id`, and `op` is `add`, `convert`, `copy`, `delete`, `restore`, `revert`, `set` or `assign`, with the edit's fields beside it) |
| `openxer_check_schedule`, `openxer_paths`, `openxer_compare`, `openxer_lookahead`, `openxer_milestones` | no | `check-schedule`, `paths`, `compare`, `lookahead`, `milestones` |
| `openxer_schedule_stats`, `openxer_resource_loading`, `openxer_wbs_summary` | no | `schedule-stats`, `resource-loading`, `wbs-summary` |
| `openxer_skills`, `openxer_skill` | no | `skills list`, `skills show` |

Prompts: one per skill (`prompts/list`, `prompts/get` with the skill id as `name`); the message is the skill's instructions after the rules every skill follows. The list is read from the skill folder on each request.

A new agent asked "Use OpenXER to inspect my schedule" has no file name. `openxer_find_schedules` (no arguments; the server's instructions say to call it first) answers with the OpenXER workspace on this PC, found by the app's own rule (`OPENXER_DATA_DIR`; an installed copy's `%LOCALAPPDATA%\OpenXER\openxer.db`, on other systems `$XDG_DATA_HOME/OpenXER/openxer.db` or `~/.local/share/OpenXER/openxer.db`; a portable copy's `openxer.db` beside the executables) plus the other kind of copy's workspace when there is one, each with its projects most recently saved first (`workspace_id`, `project_id`, `name`, `source_name`, `saved_utc`; at most 100, with `project_count`). `folder` (an absolute path) also lists the `.xer` files and workspaces directly in that folder, newest first. It only reads: workspaces are opened read-only and nothing is written. The agent then passes the workspace as `input` with a `workspace_id`.

`tools/list` gives strict JSON schemas and read and write annotations. Arguments are the CLI's JSON request without its `command` field; the edit tools take the operation's fields at the top level next to `input`, `output`, `project_id`, `workspace_id`, `dry_run` and `recalculate`. Results carry the same JSON envelope in both `structuredContent` and the text content. A failed command sets `isError`; malformed arguments return JSON-RPC `-32602`.

The server implements the [2026-07-28 stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio), including self-contained request metadata, `server/discover` and completed results. A discovery request:

```json
{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

`tools/list`, `tools/call`, `prompts/list` and `prompts/get` requests carry the same `_meta` fields on **every request**. An unsupported version returns `-32022` with `requested` and `supported`, as the [protocol schema](https://modelcontextprotocol.io/specification/2026-07-28/schema) specifies.

### The chat tool set

`openxer-cli mcp --toolset chat [--snapshot FILE] [--mode ask|edit]` serves the small tool set of OpenXER's Chat window instead of every tool (plain `mcp` is unchanged). Every tool described costs the agent tokens on every turn, so the chat set is seven read tools, `openxer_load`, `openxer_records`, `openxer_earned_value`, `openxer_usage`, `openxer_documents`, `openxer_check_schedule` and `openxer_playbook`, plus `openxer_apply` in Edit mode (Ask, the default, is read-only), with short descriptions and server instructions: about 4 KB for Ask and 6 KB for Edit against about 108 KB for the full set. The tools are built from the same definitions and run through the same dispatcher as the full set:

- With `--snapshot` every call reads that project file, so the tools take no `input` (nor `project_id`, `workspace_id` or `output`). Without it each call names its `input`.
- `openxer_load` returns `task_code`, `name`, `status`, `early_start`, `early_finish` and `total_float_minutes` for 50 activities (and 50 relationships) a page unless the call asks otherwise.
- `openxer_apply` only proposes: it always runs as a dry run on the snapshot and never writes a file. The result carries the field diff with `"proposed": true`; the app applies the batch when the user approves it.
- A result over 40,000 bytes of JSON is refused with `result_too_large` and a hint to ask for less.
- Results are sent as text only, with no `structuredContent`, so nothing is sent twice.

The same tools run in-process through `chat_tools::tools`, `chat_tools::call` and `chat_tools::preamble` (see below), so a chat provider that runs tools itself behaves exactly like the MCP server.

For existing clients the server also supports the [initialize lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle) for `2025-11-25`, `2025-06-18` and `2024-11-05`: `initialize`, `notifications/initialized`, then tool requests. No project selection carries over between calls. The server offers synchronous tools and the skill prompts; it has no sampling, elicitation, resource subscriptions, asynchronous tasks or model of its own. The host application chooses the model and talks to the user.

## Connecting

`openxer-cli connect` adds this MCP server to an AI client for you; the desktop's **AI Agents** menu runs the same code. Every result uses the JSON envelope above.

```
openxer-cli connect status
openxer-cli connect all [--dry-run]
openxer-cli connect claude-desktop [--disconnect] [--dry-run]
openxer-cli connect chatgpt [--disconnect] [--dry-run]     (the same as codex)
openxer-cli connect codex [--disconnect] [--dry-run]
openxer-cli connect claude-code
openxer-cli connect prompt
openxer-cli connect test
```

- `status` lists each client (`claude-desktop`; `codex`, named "ChatGPT desktop and Codex"; `claude-code`) with `found` (installed on this PC), its config files, the `openxer` entry's `state` and the one `action` to take. States: `connected`; `not_connected`; `other_copy` (the entry runs `mcp` with another `openxer-cli` that exists, with `installed` true when the installer's `openxer.installed` marker is beside it); `stale` (the entry runs a file that no longer exists, or other arguments); `malformed` (the file cannot be parsed). Actions, which the desktop's dialog shows as its one button: `connect` (found, not connected), `connected`, `use_this_copy` (`other_copy`), `repair` (`stale`), `not_installed` (not found and no entry), `fix_file` (`malformed`) and `copy_command` (Claude Code found). `cli_installed` says whether this `openxer-cli` is an installed copy.
- `claude-desktop` edits `claude_desktop_config.json` in `%APPDATA%\Claude\` and, for a Microsoft Store install, in each `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\` (a Store copy that does not exist yet starts from the classic file, so its other servers stay visible). The entry is `"mcpServers": {"openxer": {"command": "<this openxer-cli>", "args": ["mcp"]}}`. Claude Desktop reads it only when it starts: quit it from its tray icon (closing the window is not enough) and open it again.
- `codex` (or `chatgpt`) edits `%USERPROFILE%\.codex\config.toml` (or `%CODEX_HOME%\config.toml`), shared by the Codex app and CLI and, as reported for Windows, by the ChatGPT desktop app: table `[mcp_servers.openxer]` with `command` and `args = ["mcp"]`. The path is written as a TOML literal string (`command = 'C:\...'`) so no backslash is ever read as an escape (a basic string, escaped, only when the path holds a single quote). It counts as found when `.codex` exists, `codex` is on `PATH`, `%LOCALAPPDATA%\OpenAI\Codex` exists or a Store package `OpenAI.ChatGPT*` or `OpenAI.Codex_*` is installed. In the ChatGPT desktop app local tools are reported to work in its Work and Codex modes; plain Chat may not use them.
- `all` connects every app that is found and not connected or needs repair (`connect` and `repair` actions), one after another; one failing does not stop the others. An app connected to another copy of OpenXER is never switched by `all`: run its own command for that. Each result has `client`, `ok`, `message` and `files`.
- Only the `openxer` entry is added, updated (the command and arguments are replaced in place; other fields of the entry such as `env` stay) or removed (`--disconnect`). Every other byte of the file (other servers, settings, comments, spacing, key order, line ends, a byte order mark) is kept as it was. Running `connect` on an app connected to another copy switches it to this copy ("Use this copy"), and on one that needs repair repairs it.
- Before a file changes it is copied to `<file>.openxer-backup-<YYYYMMDD-HHMMSS>` (UTC). The new text is written atomically, read back and checked (our entry in its new state, everything else unchanged); a failed check puts the original back. Missing folders and files are created.
- A config that cannot be parsed is never written: the command fails with `config_malformed`. A read-only config is never written either (`config_read_only`). Fix the file, or add the entry by hand from the example under MCP.
- `--dry-run` returns each file's new text as `preview` and writes nothing.
- `claude-code` prints the command to run yourself (`claude mcp add --scope user openxer -- "<path>" mcp`); OpenXER never runs it.
- `prompt` prints (as `prompt`) a short setup prompt to paste into any AI tool that can run commands: the quoted path of this `openxer-cli`, to run `--help`, to add an MCP server named `openxer` that runs it with the argument `mcp` (or to call the CLI directly, as every command prints JSON), to check it with `connect test`, the safety rules (inputs never changed, edits go to new files, `--dry-run` to preview) and a first request. The desktop's **Copy Setup Prompt** in Connect AI Agents copies the same text.
- `test` starts `openxer-cli mcp`, initializes a session, lists the tools and stops the server: `"message": "Working: N tools"`.
- After a connect, `message` and `next_step` say what to do in the app and the first thing to ask ("Use OpenXER to inspect my schedule."). The command written is the `openxer-cli` you ran; the desktop uses the copy next to `OpenXER.exe` (or `OPENXER_CLI`).

Errors: `cli_missing`, `config_malformed`, `config_read_only`, `config_unknown`, `validation_failed`, `test_failed`, `unsupported`, `io_error`, `invalid_arguments`. There is no MCP tool for this: an agent does not rewrite its own client's config.

## Chat agent host

`agent_host` is what the desktop's Chat window drives: it starts an AI agent already installed on the PC (Claude Code or Codex) as a child process, which uses its own sign-in. OpenXER holds no key and makes no network calls itself. The agent gets only the chat tool set above: no command-running or file tools, the user's own agent settings, hooks, instruction files and other MCP servers ignored, and in Ask mode no edit tool.

- Claude Code runs as `claude -p --restricted --strict-mcp-config --disable-slash-commands --tools "" --input-format stream-json --output-format stream-json --include-partial-messages --permission-prompts none --mcp-config <the chat server> --allowedTools <the chat tools> --system-prompt <OpenXER's rules>`, with `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1` and `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` so no CLAUDE.md file and no auto memory is read. One process serves the whole conversation (each message is one more stdin line; after a cancel, `--resume <session id>`). `--model sonnet` is the lower cost choice.
- Codex runs as `codex exec --json --skip-git-repo-check --ignore-user-config --ignore-rules ... -- -` with `sandbox_mode="read-only"`, `approval_policy="never"`, web search off, `project_doc_max_bytes=0` (no AGENTS.md from the work folder or above it), its command, sub-agent, plugin, app, skill, browser and image features disabled, `model_instructions_file` holding OpenXER's rules in place of its own, and the chat server as `mcp_servers.openxer` (with 30 seconds to start); each later turn is `codex exec resume ... -- <thread id> -`. `-m gpt-6-luna` with low reasoning effort is the lower cost choice. Codex still reads `AGENTS.override.md` or `AGENTS.md` in its own home folder (`CODEX_HOME`, by default `%USERPROFILE%\.codex`): no documented setting turns that off, so `detect` reports such a file as `user_instructions` for the window to mention. What Codex keeps of its own tools: a script tool that runs JavaScript with no file system or network and only calls other tools (the OpenXER tools, MCP resource listing, the clock and `apply_patch`, whose writes the read-only sandbox with `approval_policy="never"` refuses), and its sub-agent tools, whose agents run with the same settings.
- Messages, the preamble and every other piece of schedule or user text go to the agent on stdin, never on a command line; the command line holds only fixed flags and paths the host makes, and model names (letters, digits and `.-_:/[]`) and ids the agent gave (UUIDs) are checked first. A `.cmd` or `.bat` install is started by the standard library, which quotes for batch files and refuses what it cannot pass safely (error `Failed`, with what to do).
- `Session::start(SessionOptions, Arc<dyn Spawner>)` gives the session and a receiver of `Event`s (`Ready`, `TextDelta`, `Message`, `ToolStarted`, `ToolFinished`, `ProposedChanges`, `TurnFinished` with the turn's own token usage and, for Claude Code, the turn's cost, and `Error`). `send`, `cancel` and `end` never block. `cancel` stops the agent and sends the cancelled `TurnFinished` at once. Every child runs in a Windows job object, so stopping it, ending the session or dropping it ends the agent and every process it started. Tool and proposal ids carry the process they came from, so they are unique in a session. Output is read as bytes; invalid UTF-8 never stops the reading. An `openxer_apply` call the agent makes becomes `ProposedChanges` with the batch and its diff; `chat_tools::apply_proposal(&mut ProjectData, changes)` applies it exactly as it was previewed (the chat apply takes no `recalculate` of its own: only the document's counts).
- Both agents keep their own copy of a conversation, tool results with schedule data included: Claude Code under `%USERPROFILE%\.claude\projects\<work folder>\`, Codex under `%USERPROFILE%\.codex\sessions\` (both are needed to resume after a cancel or for a later turn). Ending or dropping a session deletes them with the agents' own commands, `claude purge -y <work folder>` (only for a work folder the session made itself) and `codex delete --force <thread id>`, on a background thread (`forget_conversation` runs them on their own), and removes the conversation's files: its work folder when the session made it (with the agent config and a snapshot inside it). `SessionOptions::keep_files` keeps everything.
- `detect` reports whether each agent is installed (its executable on `PATH` from an injected `connect::Env`), its version and whether it is signed in (`claude auth status --json`, `codex login status`). Each check is stopped after 10 seconds, so `detect` may take up to 20 seconds and `detect_all` 40 (normally about a second); `detect_all_in_background` returns at once with a receiver the window polls. `SignIn::start` runs the agent's own sign-in (`claude auth login`, `codex login`), which opens the browser.
- The instructions every assistant gets are `chat_prompt::system_prompt(mode, user)`: a scheduling core of about 1,300 characters (`chat_prompt::CORE`: work from the schedule, think in logic and float, propose the smallest edits and wait for approval, write for the field, load the matching playbook), the user's standing instructions, and the mode line. Claude Code reads them with `--system-prompt-file` and Codex with `model_instructions_file`, from a file in the work folder, never from the command line. The methods are playbooks the assistant fetches with `openxer_playbook` only when it needs one.
- The user's standing instructions live in `chat\AGENTS.md` in the OpenXER data folder (`chat_prompt::data_folder`, the folder of the workspace, so `OPENXER_DATA_DIR` moves it too). `chat_prompt::read_user_instructions(&path)` returns their text without `<!-- -->` comments, at most 4,000 characters with a flag when cut, or none for a missing, empty or comment-only file; UTF-8 with or without a byte order mark and UTF-16 with one are read, anything else leniently. `chat_prompt::create_user_instructions(&path)` writes a short commented starter when the file does not exist and never touches one that does. OpenXER passes this file in on purpose; the agents' own discovery of instruction files stays off.
- `chat_tools::preamble(&ProjectData, &selected)` writes the few lines the first message carries (project, data date, scheduled and must finish dates, counts, the selected activities), so the agent rarely needs a discovery call.
- `edit::apply_changes(&mut ProjectData, changes, recalculate, &EvContext)` applies a changes document to a project in memory through the same parsing and setters as `apply`, all or nothing.

Process starts go through the `Spawner` trait, so the tests run every path against a scripted fake agent and the parsers against trimmed recordings of real runs on the synthetic sample (`tests/fixtures/agent/`). `tests/agent_live.rs` holds ignored tests that run the real agents on the sample: an answer and a proposed edit over two turns, the stored copies deleted at the end, and no instruction file of the user's read.

## In-process use and tests

`dispatch(Command)` runs the same file workflow as the CLI and MCP. `schedule_project(&mut ProjectData, &SchedulePatch)` and `edit_relationship(&mut ProjectData, &RelationshipEdit)` work on a project already in memory; they validate and work on a copy before committing. Writing files is then the caller's job.

Run `cargo test -p openxer-automation`. The tests cover CLI and native schedule parity, every edit operation in both rules and exact modes, dry runs, `meta`, CLI and MCP building the same edits, cycle refusal, source preservation, workspace and encoding round trips, all-or-nothing batches, completed-date visibility, paging, and MCP through a real child process.

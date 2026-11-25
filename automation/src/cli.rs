//! CLI argument parsing; execution uses the same dispatcher as MCP.
use crate::{parse_command, AutomationError, Command, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const HELP: &str = r#"OpenXER native automation

Usage:
  openxer-cli inspect --input FILE
  openxer-cli workspace migrate --input WORKSPACE.db (--output NEW.db | --dry-run)
  openxer-cli workspace dictionary --input WORKSPACE.db [--table KIND]
  openxer-cli workspace dictionary-usage --input WORKSPACE.db --table KIND
                    (--id N | --name NAME)
  openxer-cli workspace preference --input WORKSPACE.db
                    [--name new_activity_wizard|new_resource_wizard
                     --value true|false (--output NEW.db | --dry-run)]
  openxer-cli import --input FILE.xer|FILE.xml --output NEW.db [--project ID]
                    [--format xer|xml]
                    [--workspace EXISTING.db [--action new|update|replace|add]
                     [--target-workspace-id ID] [--config TYPE=ACTION,...]
                     [--dictionary KIND=copy|keep|update,...]]
  openxer-cli load --input FILE [--project ID | --workspace-id ID]
                  [--offset N] [--limit N]
                  [--relationship-offset N] [--relationship-limit N]
                  [--fields KEY,KEY] [--activities ID,ID] [--wbs PATH]
                  [--critical [true|false]] [--sort [-]FIELD]
  openxer-cli check-schedule --input FILE [--check NAME [--limit N]]
                  [--project ID | --workspace-id ID]
  openxer-cli playbook [--name NAME]
  openxer-cli curves --input FILE [--project ID | --workspace-id ID]
  openxer-cli usage --input FILE (--activity ID | --resource SHORT_NAME)
                   [--interval day|week|month] [--from DATE] [--to DATE]
                   [--costs [true|false]] [--project ID | --workspace-id ID]
  openxer-cli schedule --input FILE --output NEW_FILE [--project ID]
                      [--workspace-id ID] [--options JSON]
                      [--data-date "YYYY-MM-DD HH:MM"] [--level [true|false]]
  openxer-cli relationship add|update|remove --input FILE --output NEW_FILE
                      --predecessor CODE --successor CODE --type FS|SS|FF|SF
                      [--lag-minutes N] [--new-type FS|SS|FF|SF]
                      [--project ID] [--workspace-id ID]
  openxer-cli export --input FILE --output NEW.xer|NEW.xml [--project ID]
                    [--workspace-id ID] [--version 19.12] [--projects ID,ID]
                    [--format xer|xml]
  openxer-cli export-xlsx --input FILE --output NEW.xlsx [--project ID]
                    [--workspace-id ID] [--layout modern|tables] [--unit days|hours]
                    [--columns KEY,KEY | --template NAME|FILE.json]
  openxer-cli export-pdf --input FILE --output NEW.pdf [--project ID]
                    [--workspace-id ID] [--layout NAME] [--view gantt|network]
                    [--paper NAME] [--orientation portrait|landscape|automatic]
  openxer-cli export-scurve --input FILE --output NEW.xlsx [--project ID]
                    [--workspace-id ID] [--measure auto|labor|cost|duration]
                    [--period day|week|month|financial]
                    [--breakdown total|resource|wbs1|wbs2] [--late [true|false]]
                    [--week-start sunday|monday]
  openxer-cli export-earned-hours --input FILE --output NEW.xlsx [--project ID]
                    [--workspace-id ID] [--breakdown resource|wbs|activity]
  openxer-cli import-xlsx --input FILE --spreadsheet FILE.xlsx
                    (--output NEW_FILE | --dry-run) [--project ID]
                    [--workspace-id ID]
  openxer-cli apply --input FILE --changes CHANGES.json
                    (--output NEW_FILE | --dry-run) [--project ID]
                    [--workspace-id ID] [--recalculate true|false]
  openxer-cli set-activity --activity ID --field KEY --value VALUE  [edit flags]
  openxer-cli add-activity [--activity ID] [--name TEXT] [--wbs PATH]
                    [--selected ID]                                 [edit flags]
  openxer-cli delete-activity --activity ID                         [edit flags]
  openxer-cli dissolve-activity --activity ID [--retain-lag true|false] [edit flags]
  openxer-cli assign --activity ID --resource SHORT_NAME            [edit flags]
  openxer-cli unassign --activity ID --resource SHORT_NAME          [edit flags]
  openxer-cli set-assignment --activity ID --resource SHORT_NAME
                    --field KEY --value VALUE                       [edit flags]
  openxer-cli calendar-day --calendar NAME --date YYYY-MM-DD
                    --shifts '[["08:00","12:00"],...]'|nonwork|standard [edit flags]
  openxer-cli calendar-weekday --calendar NAME --weekday 0-6 (0 = Sunday)
                    --shifts '[["08:00","12:00"],...]'|nonwork      [edit flags]
  openxer-cli set-project --field data_date|must_finish --value DATE [edit flags]
  openxer-cli add-wbs --wbs PATH [--name TEXT]                      [edit flags]
  openxer-cli add-document --title TEXT [--parent DOC] [--reference TEXT] [edit flags]
  openxer-cli set-document --document DOC --field KEY --value VALUE [edit flags]
  openxer-cli delete-document --document DOC                        [edit flags]
  openxer-cli link-document --document DOC (--activity ID | --wbs PATH)
                    [--work-product true|false]                     [edit flags]
  openxer-cli unlink-document --document DOC (--activity ID | --wbs PATH) [edit flags]
  openxer-cli add-expense --activity ID [--name TEXT] [--fields JSON]  [edit flags]
  openxer-cli add-issue|add-risk [--name TEXT] [--fields JSON]      [edit flags]
  openxer-cli add-threshold [--parameter NAME] [--fields JSON]      [edit flags]
  openxer-cli set-expense|set-issue|set-risk|set-threshold
                    --expense|--issue|--risk|--threshold REF
                    --field KEY --value VALUE                       [edit flags]
  openxer-cli delete-expense|delete-issue|delete-risk|delete-threshold
                    --expense|--issue|--risk|--threshold REF        [edit flags]
  openxer-cli assign-risk|unassign-risk --risk REF --activity ID    [edit flags]
  openxer-cli monitor-thresholds [--from DATE] [--to DATE] [--today DATE] [edit flags]
  openxer-cli update-progress --data-date DATE [--activities ID,ID]
                    [--remaining always|duration_type]              [edit flags]
  openxer-cli apply-actuals --data-date DATE [--remaining always|duration_type] [edit flags]
  openxer-cli recalculate-costs [--active-only true] [--sync-overtime true] [edit flags]
  openxer-cli add-step --activity ID --name TEXT [--at N]           [edit flags]
  openxer-cli set-step --activity ID --step N|NAME --field KEY --value VALUE [edit flags]
  openxer-cli delete-step --activity ID --step N|NAME               [edit flags]
  openxer-cli move-step --activity ID --step N|NAME --to N          [edit flags]
  openxer-cli add-steps-from-template --activity ID --template NAME [--after N|NAME]
                                                                    [edit flags]
  openxer-cli create-step-template --activity ID --name TEXT        [edit flags]
  openxer-cli set-feedback --activity ID [--notes TEXT] [--review-required true|false]
                                                                    [edit flags]
  openxer-cli top-down-estimate --labor-units N [--wbs PATH] [--not-started-only true|false]
                    [--weights '{"PATH":WEIGHT,...}']               [edit flags]
  openxer-cli shift-wbs --wbs PATH --direction up|down|left|right   [edit flags]
  openxer-cli rename-wbs-code --wbs PATH --code TEXT                [edit flags]
  openxer-cli copy-calendar --calendar NAME [--source NAME] [--kind project|global|resource]
                                                                    [edit flags]
  openxer-cli rename-calendar --calendar NAME --name TEXT           [edit flags]
  openxer-cli set-default-calendar --calendar NAME                  [edit flags]
  openxer-cli convert-calendar --calendar NAME --to global|shared|personal [edit flags]
  openxer-cli global-change (--name SAVED_NAME | --rule JSON)       [edit flags]
      edit flags: --input FILE (--output NEW_FILE | --dry-run) [--project ID]
                  [--workspace-id ID] [--recalculate true|false]
  openxer-cli eps list --input WORKSPACE.db
  openxer-cli eps add-folder --name TEXT [--eps-id ID] [--parent EPS_ID]   [eps flags]
  openxer-cli eps rename-folder --folder EPS_ID [--name TEXT] [--new-eps-id ID] [eps flags]
  openxer-cli eps move-folder --folder EPS_ID [--parent EPS_ID]            [eps flags]
  openxer-cli eps delete-folder --folder EPS_ID [--delete-projects]        [eps flags]
  openxer-cli eps file-project --workspace-id N --folder EPS_ID            [eps flags]
  openxer-cli eps create-project --folder EPS_ID --project-id ID
                    --planned-start DATE [--name TEXT] [--must-finish DATE]
                    [--manager NAME] [--rate-type COST_PER_QTY..COST_PER_QTY5] [eps flags]
      eps flags: --input WORKSPACE.db (--output NEW.db | --dry-run)
  openxer-cli baseline list --input WORKSPACE.db [--workspace-id N]
  openxer-cli baseline add                                          [baseline flags]
  openxer-cli baseline convert --project N                          [baseline flags]
  openxer-cli baseline copy|delete|restore|revert --baseline REF    [baseline flags]
  openxer-cli baseline set --baseline REF [--baseline-id ID] [--name TEXT]
                    [--type NAME|none]                              [baseline flags]
  openxer-cli baseline assign [--project-baseline current|REF]
                    [--primary|--secondary|--tertiary current|none|REF] [baseline flags]
  openxer-cli baseline update --baseline REF [--options JSON] [--activities ID,ID]
                                                                    [baseline flags]
      baseline flags: --input WORKSPACE.db --workspace-id N (--output NEW.db | --dry-run)
  openxer-cli documents --input FILE [--project ID] [--workspace-id ID]
                    [--activity ID | --wbs PATH]
  openxer-cli read-document --input FILE --document DOC [--project ID]
                    [--workspace-id ID] [--location private|public]
                    [--offset N] [--limit N] [--page N] [--pages N]
  openxer-cli records --input FILE --kind expenses|issues|risks|thresholds
                    [--project ID] [--workspace-id ID] [--activity ID]
  openxer-cli earned-value --input FILE [--basis cost|labor_units]
                    [--project ID] [--workspace-id ID] [--activity ID]
  openxer-cli check-schedule --input FILE [--long-lag-hours N]
                    [--large-float-hours N] [--large-duration-hours N]  [read flags]
  openxer-cli paths --input FILE [--activity ID] [--count 1-10]      [read flags]
  openxer-cli compare --input FILE (--reference FILE [--reference-project ID]
                    [--reference-workspace-id ID] | --baseline project|primary)
                                                                    [read flags]
  openxer-cli lookahead --input FILE [--weeks 1-26] [--from DATE]   [read flags]
  openxer-cli milestones --input FILE                               [read flags]
  openxer-cli schedule-stats --input FILE [--update-period-days N]  [read flags]
  openxer-cli resource-loading --input FILE [--interval day|week|month] [read flags]
  openxer-cli wbs-summary --input FILE [--wbs-level 1-10]           [read flags]
      read flags: [--project ID | --workspace-id ID]
  openxer-cli skills list | show ID | validate [FILE] | new ID | path
  openxer-cli call --json JSON
  openxer-cli call --request-file REQUEST.json
  openxer-cli connect status
  openxer-cli connect all [--dry-run]
  openxer-cli connect claude-desktop|chatgpt|codex [--disconnect] [--dry-run]
  openxer-cli connect claude-code
  openxer-cli connect prompt
  openxer-cli connect test
  openxer-cli find-schedules [--folder DIR]
  openxer-cli mcp [--toolset chat [--snapshot FILE] [--mode ask|edit]]
  openxer-cli --help | --version

Commands read the source without changing it and refuse existing output files.
Choose --project for a project ID (the XER proj_id), or --workspace-id for an OpenXER saved row.
A workspace keeps its global calendars, resources, roles, global and EPS codes,
UDFs, cost accounts and the other dictionaries once for all of its projects
(workspace schema 3): commands that read a workspace project add them back,
commands that write one merge them back, so an edit of a global calendar or a
resource reaches every project. An exported XER holds the dictionary rows its
projects use. workspace dictionary lists the dictionary with the projects
using each row; workspace dictionary-usage answers for one row (KIND:
calendar, resource, role, activity_code, code_value, udf, account or a table
name). A workspace made by an earlier version (schema 1) is read and written
as before; workspace migrate converts a copy of it (--dry-run first).
workspace preference reads the app's User Preferences, Assistance switches
kept in a workspace: new_activity_wizard (on by default; Edit > Add opens the
New Activity wizard, off inserts the row at once) and new_resource_wizard
(on by default; the Resources view's Add opens the New Resource wizard).
With --name and --value it writes a copy with the switch set.
import --workspace matches the file's dictionary rows to the workspace's by
name: the same content is reused, other values are added as <name>_<n> unless
--dictionary says keep (use the workspace's) or update (the workspace's takes
the file's values); each project lists these decisions in "dictionary".
import --config sets per-type actions for --action update, for example
activities=update+delete,relationships=update,calendars=keep. Types: activities,
relationships, wbs, assignments, calendars, resources, activity_codes, udfs,
notebooks. Actions: insert, keep, update, skip. export-xlsx --template runs a saved
export template (columns, filter, sort and options) by its name in the workspace
input, or from a template .json file; it replaces --columns and --unit. import-xlsx --dry-run prints the
preview (changed cells, new rows, deletes, errors) without writing anything.
curves lists the resource curves an assignment can use, read-only: the file's
own curves and OpenXER's preset curves ("Even", "Early Weighted", "Late
Weighted", "Ramp Up", "Ramp Down", "Bell, Centered", "Bell, Early Peak",
"Bell, Late Peak", "Two Peaks", "Steady with Ramps"), each with its 21 points. set_assignment field curve
takes a curve name (or null to clear); a preset is copied into the file the
first time an assignment uses it. Curves shape how an assignment's units and
costs spread over time (usage views, planned value, usage) on Fixed Duration
activities; the exact numbers follow OpenXER's reading of the documented
behavior and are not yet measured against other software.
usage reads an activity's or a resource's assignments period by period
(read-only): budgeted, actual, remaining and remaining late units (or costs
with --costs true) per day, week (from Monday, the default) or month, each
assignment's windows, curve and whether it shapes the spread, and the totals.
Each series gives total, per_period and unplaced, the part of the stored
quantity no period holds (outside the periods, or nothing to spread over):
total + unplaced = stored.
export-scurve writes the S-curve workbook of Tools > Reports > S-Curve: per
period the budgeted (from the assigned project or primary baseline, else the
current plan), actual (to the data date) and remaining values, running sums and
percents as formulas, earned to the data date and a native chart, the
breakdown matrices and an Info sheet. --measure auto (the default) takes labor
hours when labor assignments carry budgeted units, else cost (assignments and
expenses), else duration (each activity weighs its planned working days);
--period week (from Sunday) and --breakdown resource (total for duration) are
the defaults; --late adds actual plus remaining on the late dates.
export-earned-hours writes cumulative earned labor hours to the data date:
baseline budgeted units x performance % complete per labor assignment, with
actual, remaining and at completion, by resource (default), wbs or activity,
rows sorted by ID so two exports subtract in Excel; a project without labor
hours fails with no_labor_units and writes nothing.
export-pdf prints with the desktop's renderer: it runs the OpenXER executable
next to openxer-cli (or OPENXER_DESKTOP) headless and uses the layout's Page
Setup.
apply runs an openxer-changes/1 file: {"format":"openxer-changes/1",
"recalculate":true, "operations":[{"op":"set_activity",...},...]}. Operations run
in file order, all or nothing; nothing is written when one fails. Operations:
set_project, add_wbs, add_activity, delete_activity, dissolve_activity
(activity, retain_lag; needs a predecessor and a successor), set_activity,
assign_resource, remove_assignment, set_assignment, relationship (add, edit or
remove, with lag_hours and comments), calendar_day, calendar_weekday, schedule,
and the record edits set_resource_limit, set_activity_code, set_project_code
(code_type, value: the project's value of a project code), add_code_type
(code_type, scope project|eps|global, max_length), add_code_value, set_code_value,
delete_code_value, set_notebook, set_wbs_notebook, set_udf, add_calendar,
set_calendar, delete_calendar, add_resource, set_resource, set_wbs,
set_schedule_option, add_project_funding (source, amount, share),
set_project_funding (source, field source|amount|share, value),
delete_project_funding (source), delete_resource (resource),
delete_financial_period (period, calendar; refused while stored period
performance refers to the period), global_change (Tools > Global Change on every
activity: name of a definition saved in a workspace, or rule {match all|any,
conditions, then, otherwise}; the result lists the report lines, activity,
field, old, new and exception, and changed counts the values written; durations
in hours), merge_resources
(from: [short names], into: short name; assignments, rates, codes, UDF values,
role links, child resources and the calendar move to into, assignments of one
activity are combined, then from is removed; --dry-run previews it) and
set_wbs_earned_value (wbs, performance activity_percent|wbs_milestones|0/100|
50/50|custom, custom_percent, estimate_to_complete remaining|pf_1|pf_cpi|
pf_cpi_spi|pf, performance_factor), and the document edits add_document (title,
parent, reference, fields {...}), set_document (document, field title|reference|
deliverable|status|category|author|version|date|private_location|public_location|
description, value), delete_document (document; the documents under it and their
links go too), link_document and unlink_document (document, activity or wbs,
work_product), and the register edits add_expense (activity, name, fields),
set_expense, delete_expense, add_issue, set_issue, delete_issue, add_risk,
set_risk, delete_risk, assign_risk, unassign_risk, add_threshold (parameter,
fields), set_threshold, delete_threshold and monitor_thresholds (from, to,
today), the Tools edits update_progress (data_date, activities, remaining
always|duration_type), apply_actuals (data_date, remaining),
recalculate_costs (active_only, sync_overtime) and top_down_estimate
(labor_units, wbs, not_started_only, weights {path: weight}), the step edits
add_step (activity, name, at), set_step (activity, step, field name|complete|
percent_complete|weight|description, value), delete_step, move_step (to),
add_steps_from_template (template, after), create_step_template (name),
set_feedback (activity, notes, review_required), shift_wbs (wbs, direction
up|down|left|right), rename_wbs_code (wbs, code), copy_calendar (calendar,
source, kind project|global|resource), rename_calendar (calendar, name),
set_default_calendar and convert_calendar (to global|shared|personal).
set_activity also takes the Status tab units and costs: budgeted_, actual_,
remaining_, at_completion_ or this_period_ with labor_units,
nonlabor_units, labor_cost, nonlabor_cost or material_cost; set_assignment also takes
original_duration, remaining_duration (hours), drive_activity_dates,
resource (replace), role and cost_account (by name; null clears). Any
operation may carry a "meta" object
(provenance such as source or evidence) that apply ignores. Activities are named by
Activity ID, resources by short name, calendars by name and WBS elements by
their code path (1.2.3). Durations and lags are hours, dates project-local
"YYYY-MM-DD HH:MM", choices take XER codes (TT_Task, CS_ALAP, DT_FixedDUR2) or
display names, and a null value clears. Edits use the desktop's setters, so
statuses, durations and assignments follow the same rules. --recalculate false
(or "recalculate": false, also per operation) writes set_activity and
set_assignment values exactly into their XER columns with no duration type,
units or cost knock-on; that mode also takes derived fields such as
remaining_units_per_hour, costs and raw XER column names. The single edit
commands run one operation the same way; --dry-run returns the field diff
without writing. --value is the text typed (1.10 stays 1.10; numeric fields
read the number from it), except null (clears) and true or false; --value ""
and --notes "" give empty text, and a value starting with -- is given inline
(--value=--x). --review-required, --active-only, --sync-overtime,
--not-started-only, --work-product and --costs mean true on their own.
eps list reads an OpenXER workspace and returns its EPS folders (EPS ID, name,
parent, path, projects filed directly in each) and every project, each with its
rollup: start and finish (earliest start and latest finish, actual else early),
late start and finish, data date (a folder shows the latest), Must Finish By,
baseline finish and finish_variance_hours (baseline finish minus finish in
working hours), total_float_hours (the lowest of the unfinished activities; a
folder the lowest of its projects), schedule_percent_complete (duration based:
planned span minus remaining span over planned span, in working time),
labor_units_hours and costs (budgeted, actual, remaining; they add up) and the
activity counts. Values come from the stored projects, so closed projects are
current. The eps edits work on a copy written to --output: move-folder without
--parent moves the folder to the top level; delete-folder deletes the folder,
the folders under it and every project filed in them with their baselines (a
folder that holds projects needs --delete-projects; --dry-run lists what would
go without it) and refuses to delete the last top-level folder; create-project uses the New Project
wizard's rules (Project ID unique, up to 20 characters; Planned Start required;
a date without a time takes 08:00, Must Finish By 17:00) and the responsible
managers of the project last open in the app.
baseline list shows each workspace project's stored baselines as Project >
Maintain Baselines lists them (id, Baseline ID, name, type, data date, date
added, last update date, what each is assigned as) and its Assign Baselines
choices. The baseline edits work on a copy written to --output with the app's
rules: add saves a copy of the project as "<project ID> - B<n>" /
"<project name> - B<n>" (n the lowest free number); convert makes the closed
workspace project --project a baseline that keeps its own ID and name, with a
blank last update date; copy makes another baseline with the next number and
the source's last update date; delete and restore refuse a baseline assigned
as project, primary, secondary or tertiary baseline (error code refused);
restore makes the baseline a separate project with the same ID and name in
the owner's EPS folder; revert (Revert Project to Baseline) gives the project
the baseline's data and keeps its ID, name and baselines; set changes the
Baseline ID, name or type; assign sets the project baseline (current or a
baseline) and the primary, secondary and tertiary baselines (current, none
or a baseline). REF is a baseline's id from baseline list, its Baseline ID or
its name. update (Maintain Baselines > Update) carries the project's data
into the baseline by the Update Baseline options: --options is a JSON object
of them by name (omitted ones take the dialog's defaults; see
openxer_baseline_update), --activities limits the activities added or updated
to those Activity IDs; --dry-run true returns the preview (each changed
project, WBS element and activity: Added, Deleted or Updated, with field,
project value and baseline value) and writes nothing; the update sets Last
Update Date and writes a log to the options' log_path when one is given.
documents lists the project's work products and documents (title, reference,
version, status, category, locations, description, linked activities and WBS
elements), or only those linked to --activity or --wbs; load lists each
activity's linked documents too. A document (DOC) is named by its reference
number, title or document id. read-document returns the text of the file a
document points at (--location private or public; by default private when set):
only an existing file on a local drive named by an absolute path, as Launch in
the WPs & Docs view opens, under the input file's folder or a folder listed in
the environment variable OPENXER_DOCUMENT_ROOTS (separated by ; on Windows, :
elsewhere); relative, network (UNC) and web locations are
refused, and so are programs and scripts (.exe .bat .cmd .ps1 .js .vbs .msi .com
.scr). txt, md, csv, json, xml and log files come back as text and docx as its
body text, paged by --offset and --limit characters (default 20000, at most
200000); PDF text is extracted page by page, --page (from 1) and --pages (default
10, at most 100). Files over 64 MB are not read. Error codes: document_not_found,
no_location, location_refused, refused_type, unsupported_type, file_too_large,
read_failed.
records lists the project's expense items, issues, risks or thresholds with
every field the set_ edits take (choices by label, activities by Activity ID,
WBS elements by path, resources by short name); a record (REF) is named by its
id, or by its name (a risk also by its Risk ID) when that is unique.
set_expense keeps budgeted units x price = budgeted cost and an item without
actuals all remaining; set_issue activity and wbs clear each other. Issue
fields Monitor Thresholds sets (threshold_parameter, actual_value,
lower_threshold, upper_threshold) are read-only. monitor-thresholds
evaluates every enabled threshold as Tools > Monitor Thresholds does, against
the workspace's assigned project and primary baselines (an XER input is its
own baseline), and raises or updates issues; --from and --to replace every
threshold's own window. earned-value returns BAC, PV, EV, AC, SV, CV, SPI,
CPI, ETC, EAC and VAC per activity and for the project, as the earned value
columns compute them (planned value follows resource curves the same way).
The analysis commands only read and give the numbers schedule reviews need:
check-schedule runs Check Schedule with the items each check finds;
paths ranks the chains of work into an activity (the project finish by
default) by float; compare sets the project against an earlier file or a
stored baseline (moves in calendar days); lookahead lists the work of the
next weeks by WBS, ready or waiting and on what; milestones lists every
milestone with float, baseline move and its driving ties; schedule-stats
counts activities, durations, calendars, names and WBS levels;
resource-loading gives each resource's units per period against its limit;
wbs-summary rolls the schedule up by WBS to a level.
skills manages the instructions agents follow to analyze and build
schedules: list (built-in, yours and customized, and the files left out
with their error), show ID (the instructions), validate (one file, or every
file in the folder), new ID (a file from the template in the folder) and
path (the folder: skills in the OpenXER data folder, which OPENXER_DATA_DIR
moves). A file whose id equals a built-in's replaces it; deleting the file
brings the built-in back.
JSON responses use schema_version, ok, and result or error. Exit codes:
0 success, 1 command failure, 2 invalid command-line/request arguments.
load returns every activity field unless --fields names some (task_code is
always kept; wbs, the code path, and critical, by the project's own
definition, come only when named); --activities, --wbs (the element and
everything under it) and --critical keep only the matching activities, and
then only the relationships with an end among them, with matched_activities
and matched_relationships counted and unknown Activity IDs in not_found.
--sort orders the activities by one of those fields (documents aside), with a
- first for descending (--sort=-early_finish lists the latest finish first);
WBS paths sort element by element (1.2 before 1.10); empty values go last,
and equal values keep the project's order.
--limit 0 or --relationship-limit 0 returns no rows of that kind (counts only,
with no next offset).
load --fields also names original_duration_minutes, actual_start,
actual_finish, constraint and constraint_date, given only when named.
check-schedule runs Tools > Check Schedule read-only with the app's own rules
and default targets: one row per check (Logic, Negative lags, Positive Lags,
Long Lags, Relationship Types, Hard Constraints, Soft Constraints, Large
Float, Negative Float, Large Durations, Activities with Invalid progress
dates, Resources / Cost, Late Activities, BEI, Out Of Sequence) with check,
count, of, result (a percent, or the index for BEI), target, pass and the first
five offenders; --check
NAME also lists that row's Activity IDs (relationships as PRED -> SUCC TYPE),
--limit of them (default 50, at most 500). Late Activities and BEI compare
with the project's own planned finishes. playbook lists the scheduling
playbooks (build, update, audit, critical-path, delay-recovery,
schedule-check), and --name returns one: step-by-step methods that name the
chat tools to use, for an agent to load when it needs one.
The mcp subcommand serves JSON-RPC over stdin/stdout without opening a UI.
mcp --toolset chat serves the small tool set of OpenXER's Chat window: load,
records, earned_value, usage, documents, check_schedule and playbook for
reading, and with --mode edit
(ask is the default, read-only) apply as a proposal only: it always runs as a
dry run on the snapshot and never writes a file, so the user approves the
change in the app. --snapshot names the project file every call reads, so
the tools take no input; results over the chat size limit are refused with
result_too_large and a hint to narrow the request.
connect adds OpenXER's MCP server (this openxer-cli with the argument mcp) to an
AI client's config: status reports each client with its state and the one
action to take (connect, connected, use_this_copy, repair, not_installed,
fix_file, copy_command), claude-desktop and codex (chatgpt is the same: the
ChatGPT desktop app and Codex share one config) merge only the "openxer" entry
into the client's config (backed up first as FILE.openxer-backup-TIMESTAMP,
written atomically and checked; every other byte is kept), which also repairs
an entry or switches one that runs another copy of OpenXER to this copy,
--disconnect removes only that entry, --dry-run shows the new text without
writing, all connects every found app that is not connected or needs repair
(never one connected to another copy), claude-code prints the command to run
in a terminal, prompt prints a setup prompt to paste into any AI tool that can
run commands, and test starts the MCP server and lists its tools. A config
that cannot be parsed, or a read-only one, is never changed.
find-schedules (read-only) tells an agent where the user's schedules are: the
OpenXER workspace this copy opens (and the other copy's, installed or
portable, when it has one) with its projects, most recently saved first, each
with the file it came from; --folder also lists the XER and workspace files in
one folder (not its subfolders). Pass a workspace as --input with
--workspace-id, or an XER file as --input, to the other commands.
"#;

#[derive(Debug)]
pub enum Invocation {
    Help,
    Version,
    Mcp,
    /// `mcp --toolset chat`: the Chat window's tool set.
    ChatMcp(crate::chat_tools::ChatContext),
    Connect(crate::connect::Request),
    Run(Box<Command>),
}

fn invalid(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

/// Flags that mean true on their own and take an optional true or false.
const BARE_TRUE: &[&str] = &[
    "level",
    "dry-run",
    "recalculate",
    "review-required",
    "active-only",
    "sync-overtime",
    "not-started-only",
    "work-product",
    "costs",
    "late",
    "critical",
    "delete-projects",
];

fn flags(args: &[String], allowed: &[&str]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut position = 0;
    while position < args.len() {
        let argument = &args[position];
        let name_value = argument.strip_prefix("--").ok_or_else(|| {
            invalid(format!(
                "Unexpected argument {argument:?}; use named --flags"
            ))
        })?;
        let (name, inline) = name_value
            .split_once('=')
            .map_or((name_value, None), |(name, value)| (name, Some(value)));
        if !allowed.contains(&name) {
            return Err(invalid(format!("Unknown flag --{name}")));
        }
        if result.contains_key(name) {
            return Err(invalid(format!("Duplicate flag --{name}")));
        }
        // Text that may be cleared takes an empty value; a value that starts
        // with -- is given inline (--value=--x).
        let empty_ok = matches!(name, "value" | "notes");
        let value = if let Some(value) = inline {
            if value.is_empty() && !empty_ok {
                return Err(invalid(format!("--{name} requires a value")));
            }
            value.to_owned()
        } else if BARE_TRUE.contains(&name) {
            if args
                .get(position + 1)
                .is_some_and(|next| next == "true" || next == "false")
            {
                position += 1;
                args[position].clone()
            } else {
                "true".into()
            }
        } else {
            position += 1;
            let value = args
                .get(position)
                .filter(|value| (empty_ok || !value.is_empty()) && !value.starts_with("--"))
                .ok_or_else(|| invalid(format!("--{name} requires a value")))?;
            value.clone()
        };
        result.insert(name.to_owned(), value);
        position += 1;
    }
    Ok(result)
}

fn json(text: &str, label: &str) -> Result<Value> {
    serde_json::from_str(text).map_err(|error| invalid(format!("Invalid {label} JSON: {error}")))
}

fn put_integer(object: &mut Map<String, Value>, key: &str, text: &str, signed: bool) -> Result<()> {
    let number = if signed {
        Value::from(
            text.parse::<i64>()
                .map_err(|_| invalid(format!("{key} must be a signed integer")))?,
        )
    } else {
        Value::from(
            text.parse::<u64>()
                .map_err(|_| invalid(format!("{key} must be a nonnegative integer")))?,
        )
    };
    object.insert(key.into(), number);
    Ok(())
}

/// `connect status | all | claude-desktop | chatgpt | codex | claude-code |
/// prompt | test`, with `--disconnect` and `--dry-run` for the config-editing
/// clients (`chatgpt` is `codex`: they share one config) and `--dry-run`
/// for `all`.
fn parse_connect(args: &[String]) -> Result<crate::connect::Request> {
    use crate::connect::{Client, Request};
    let what = args.first().ok_or_else(|| {
        invalid("connect requires status, all, claude-desktop, chatgpt, codex, claude-code, prompt or test")
    })?;
    let mut disconnect = false;
    let mut dry_run = false;
    for flag in &args[1..] {
        match flag.as_str() {
            "--disconnect" if !disconnect => disconnect = true,
            "--dry-run" if !dry_run => dry_run = true,
            other => {
                return Err(invalid(format!(
                    "Unexpected argument {other:?} for connect {what}"
                )))
            }
        }
    }
    let plain = |request: Request| {
        if disconnect || dry_run {
            Err(invalid(format!("connect {what} takes no flags")))
        } else {
            Ok(request)
        }
    };
    match what.as_str() {
        "status" => plain(Request::Status),
        "test" => plain(Request::Test),
        "claude-code" => plain(Request::ClaudeCode),
        "prompt" => plain(Request::Prompt),
        "all" if !disconnect => Ok(Request::All { dry_run }),
        "all" => Err(invalid("connect all takes only --dry-run")),
        id => match Client::parse(id) {
            Some(client) => Ok(Request::Apply {
                client,
                disconnect,
                dry_run,
            }),
            None => Err(invalid(format!(
                "Unknown connect target {id:?}; use status, all, claude-desktop, chatgpt, codex, claude-code, prompt or test"
            ))),
        },
    }
}

/// `mcp --toolset chat [--snapshot FILE] [--mode ask|edit] [--playbooks-dir DIR] [--ui-actions FILE]`.
fn parse_chat_mcp(args: &[String]) -> Result<crate::chat_tools::ChatContext> {
    use crate::chat_tools::{ChatContext, ChatMode};
    let fields = flags(
        args,
        &["toolset", "snapshot", "mode", "playbooks-dir", "ui-actions"],
    )?;
    match fields.get("toolset").map(String::as_str) {
        Some("chat") => {}
        Some(other) => {
            return Err(invalid(format!(
                "Unknown toolset {other:?}; the only one is chat (omit --toolset for every tool)"
            )))
        }
        None => return Err(invalid("--snapshot and --mode need --toolset chat")),
    }
    let mode = match fields.get("mode") {
        Some(text) => ChatMode::parse(text).ok_or_else(|| invalid("--mode accepts ask or edit"))?,
        None => ChatMode::Ask,
    };
    Ok(ChatContext {
        snapshot: fields.get("snapshot").map(std::path::PathBuf::from),
        mode,
        playbooks_dir: fields.get("playbooks-dir").map(std::path::PathBuf::from),
        ui_actions: fields.get("ui-actions").map(std::path::PathBuf::from),
    })
}

/// Parse arguments excluding the executable name. No scheduler, GUI, or output
/// publication runs here. `call --request-file` only reads its JSON request.
pub fn parse<I, S>(args: I) -> Result<Invocation>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(Invocation::Help);
    };
    if command == "mcp" && args.len() > 1 {
        return parse_chat_mcp(&args[1..]).map(Invocation::ChatMcp);
    }
    if matches!(
        command,
        "--help" | "-h" | "help" | "--version" | "-V" | "version" | "mcp"
    ) {
        if args.len() != 1 {
            return Err(invalid(format!(
                "{command} does not accept additional arguments"
            )));
        }
        return Ok(match command {
            "mcp" => Invocation::Mcp,
            "--version" | "-V" | "version" => Invocation::Version,
            _ => Invocation::Help,
        });
    }
    if command == "connect" {
        return parse_connect(&args[1..]).map(Invocation::Connect);
    }
    if command == "eps" {
        if args.len() >= 2 && args[1..].iter().all(|a| a == "--help" || a == "-h") {
            return Ok(Invocation::Help);
        }
        if args.len() == 3 && matches!(args[2].as_str(), "--help" | "-h") {
            return Ok(Invocation::Help);
        }
        return parse_eps(&args[1..]).map(|c| Invocation::Run(Box::new(c)));
    }
    if command == "baseline" {
        if args.len() == 3 && matches!(args[2].as_str(), "--help" | "-h") {
            return Ok(Invocation::Help);
        }
        let request = crate::baseline_cmd::parse_cli(&args[1..], flags)?;
        return parse_command(request).map(|c| Invocation::Run(Box::new(c)));
    }
    if command == "skills" {
        return parse_skills(&args[1..]);
    }
    if command == "workspace" {
        if args.len() == 3 && matches!(args[2].as_str(), "--help" | "-h") {
            return Ok(Invocation::Help);
        }
        let request = crate::workspace_cmd::parse_cli(&args[1..], flags)?;
        return parse_command(request).map(|c| Invocation::Run(Box::new(c)));
    }
    if !matches!(
        command,
        "inspect"
            | "find-schedules"
            | "import"
            | "load"
            | "check-schedule"
            | "playbook"
            | "curves"
            | "usage"
            | "schedule"
            | "relationship"
            | "export"
            | "export-xlsx"
            | "export-pdf"
            | "export-scurve"
            | "export-earned-hours"
            | "import-xlsx"
            | "documents"
            | "read-document"
            | "records"
            | "earned-value"
            | "paths"
            | "compare"
            | "lookahead"
            | "milestones"
            | "schedule-stats"
            | "resource-loading"
            | "wbs-summary"
            | "call"
            | "apply"
    ) && edit_command(command).is_none()
    {
        return Err(invalid(format!("Unknown command {command:?}; use --help")));
    }
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        return Ok(Invocation::Help);
    }
    if command == "call" {
        let fields = flags(&args[1..], &["json", "request-file"])?;
        if fields.len() != 1 {
            return Err(invalid(
                "call requires exactly one of --json or --request-file",
            ));
        }
        let request = if let Some(text) = fields.get("json") {
            json(text, "request")?
        } else {
            let path = &fields["request-file"];
            let text = std::fs::read_to_string(path)
                .map_err(|error| invalid(format!("Cannot read request file {path:?}: {error}")))?;
            json(&text, "request")?
        };
        return parse_command(request).map(|command| Invocation::Run(Box::new(command)));
    }
    if command == "apply" || edit_command(command).is_some() {
        return parse_edit(command, &args[1..]).map(|c| Invocation::Run(Box::new(c)));
    }
    let mut edit = Map::new();
    let start = if command == "relationship" {
        let action = args
            .get(1)
            .map(String::as_str)
            .filter(|action| matches!(*action, "add" | "update" | "remove"))
            .ok_or_else(|| invalid("relationship requires add, update, or remove"))?;
        if args.len() == 3 && matches!(args[2].as_str(), "--help" | "-h") {
            return Ok(Invocation::Help);
        }
        edit.insert("action".into(), Value::from(action));
        2
    } else {
        1
    };
    let allowed: &[&str] = match command {
        "inspect" => &["input"],
        "find-schedules" => &["folder"],
        "curves" => &["input", "project", "workspace-id"],
        "check-schedule" => &[
            "input",
            "project",
            "workspace-id",
            "check",
            "limit",
            "long-lag-hours",
            "large-float-hours",
            "large-duration-hours",
        ],
        "playbook" => &["name"],
        "usage" => &[
            "input",
            "project",
            "workspace-id",
            "activity",
            "resource",
            "interval",
            "from",
            "to",
            "costs",
        ],
        "import" => &[
            "input",
            "output",
            "project",
            "workspace",
            "action",
            "target-workspace-id",
            "config",
            "dictionary",
            "format",
        ],
        "load" => &[
            "input",
            "project",
            "workspace-id",
            "offset",
            "limit",
            "relationship-offset",
            "relationship-limit",
            "fields",
            "activities",
            "wbs",
            "critical",
            "sort",
        ],
        "schedule" => &[
            "input",
            "output",
            "project",
            "workspace-id",
            "options",
            "data-date",
            "level",
        ],
        "relationship" => &[
            "input",
            "output",
            "project",
            "workspace-id",
            "predecessor",
            "successor",
            "type",
            "lag-minutes",
            "new-type",
        ],
        "export" => &[
            "input",
            "output",
            "project",
            "workspace-id",
            "version",
            "projects",
            "format",
        ],
        "export-xlsx" => &[
            "input",
            "output",
            "project",
            "workspace-id",
            "layout",
            "unit",
            "columns",
            "template",
        ],
        "export-pdf" => &[
            "input",
            "output",
            "project",
            "workspace-id",
            "layout",
            "view",
            "paper",
            "orientation",
        ],
        "export-scurve" => &[
            "input",
            "output",
            "project",
            "workspace-id",
            "measure",
            "period",
            "breakdown",
            "late",
            "week-start",
        ],
        "export-earned-hours" => &["input", "output", "project", "workspace-id", "breakdown"],
        "import-xlsx" => &[
            "input",
            "spreadsheet",
            "output",
            "project",
            "workspace-id",
            "dry-run",
        ],
        "documents" => &["input", "project", "workspace-id", "activity", "wbs"],
        "records" => &["input", "project", "workspace-id", "kind", "activity"],
        "earned-value" => &["input", "project", "workspace-id", "basis", "activity"],
        "paths" => &["input", "project", "workspace-id", "activity", "count"],
        "compare" => &[
            "input",
            "project",
            "workspace-id",
            "reference",
            "reference-project",
            "reference-workspace-id",
            "baseline",
        ],
        "lookahead" => &["input", "project", "workspace-id", "weeks", "from"],
        "milestones" => &["input", "project", "workspace-id"],
        "schedule-stats" => &["input", "project", "workspace-id", "update-period-days"],
        "resource-loading" => &["input", "project", "workspace-id", "interval"],
        "wbs-summary" => &["input", "project", "workspace-id", "wbs-level"],
        "read-document" => &[
            "input",
            "project",
            "workspace-id",
            "document",
            "location",
            "offset",
            "limit",
            "page",
            "pages",
        ],
        _ => unreachable!(),
    };
    let fields = flags(&args[start..], allowed)?;
    if command == "read-document" && !fields.contains_key("document") {
        return Err(invalid("read-document requires --document"));
    }
    if fields.contains_key("project") && fields.contains_key("workspace-id") {
        return Err(invalid("Choose --project or --workspace-id, not both"));
    }
    let mut request = Map::new();
    request.insert("command".into(), Value::from(command.replace('-', "_")));
    let mut options = if let Some(value) = fields.get("options") {
        json(value, "schedule options")?
            .as_object()
            .cloned()
            .ok_or_else(|| invalid("--options must contain a JSON object"))?
    } else {
        Map::new()
    };
    for (name, value) in fields {
        match name.as_str() {
            "input" | "output" | "workspace" | "action" | "config" | "dictionary" | "version"
            | "projects" | "spreadsheet" | "layout" | "unit" | "columns" | "template" | "view"
            | "paper" | "orientation" | "activity" | "wbs" | "document" | "location" | "kind"
            | "basis" | "resource" | "interval" | "from" | "to" | "measure" | "period"
            | "breakdown" | "folder" | "sort" | "check" | "name" | "format" | "reference"
            | "baseline" => {
                request.insert(name, Value::from(value));
            }
            "long-lag-hours"
            | "large-float-hours"
            | "large-duration-hours"
            | "update-period-days" => {
                let number = value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| invalid(format!("--{name} must be a number")))?;
                request.insert(name.replace('-', "_"), Value::from(number));
            }
            "count" | "weeks" => put_integer(&mut request, &name, &value, false)?,
            // wbs-summary's level is a WBS depth (--level is schedule's
            // leveling switch).
            "wbs-level" => put_integer(&mut request, "level", &value, false)?,
            "reference-project" => put_integer(&mut request, "reference_project_id", &value, true)?,
            "reference-workspace-id" => {
                put_integer(&mut request, "reference_workspace_id", &value, true)?
            }
            "week-start" => {
                request.insert("week_start".into(), Value::from(value));
            }
            // Load's comma separated lists.
            "fields" | "activities" => {
                let list: Vec<Value> = value
                    .split(',')
                    .map(|s| Value::from(s.trim()))
                    .filter(|v| v.as_str() != Some(""))
                    .collect();
                request.insert(name, Value::Array(list));
            }
            "critical" => {
                request.insert(
                    "critical".into(),
                    Value::Bool(
                        value
                            .parse::<bool>()
                            .map_err(|_| invalid("--critical accepts true or false"))?,
                    ),
                );
            }
            "late" => {
                request.insert(
                    "late".into(),
                    Value::Bool(
                        value
                            .parse::<bool>()
                            .map_err(|_| invalid("--late accepts true or false"))?,
                    ),
                );
            }
            "costs" => {
                request.insert(
                    "costs".into(),
                    Value::Bool(
                        value
                            .parse::<bool>()
                            .map_err(|_| invalid("--costs accepts true or false"))?,
                    ),
                );
            }
            "target-workspace-id" => {
                put_integer(&mut request, "target_workspace_id", &value, true)?
            }
            "dry-run" => {
                request.insert(
                    "dry_run".into(),
                    Value::Bool(
                        value
                            .parse::<bool>()
                            .map_err(|_| invalid("--dry-run accepts true or false"))?,
                    ),
                );
            }
            "project" => put_integer(&mut request, "project_id", &value, true)?,
            "workspace-id" => put_integer(&mut request, "workspace_id", &value, true)?,
            "offset"
            | "limit"
            | "relationship-offset"
            | "relationship-limit"
            | "page"
            | "pages" => put_integer(&mut request, &name.replace('-', "_"), &value, false)?,
            "predecessor" | "successor" => {
                edit.insert(name, Value::from(value));
            }
            "type" => {
                edit.insert(
                    "relationship_type".into(),
                    Value::from(value.to_ascii_uppercase()),
                );
            }
            "new-type" => {
                edit.insert("new_type".into(), Value::from(value.to_ascii_uppercase()));
            }
            "lag-minutes" => put_integer(&mut edit, "lag_minutes", &value, true)?,
            "options" => {}
            "data-date" | "level" => {
                let key = if name == "level" {
                    "level_resources"
                } else {
                    "data_date"
                };
                if options.contains_key(key) {
                    return Err(invalid(format!("--{name} duplicates {key} in --options")));
                }
                let value = if name == "level" {
                    Value::Bool(
                        value
                            .parse::<bool>()
                            .map_err(|_| invalid("--level accepts true or false"))?,
                    )
                } else {
                    Value::from(value)
                };
                options.insert(key.into(), value);
            }
            _ => unreachable!(),
        }
    }
    if command == "schedule" {
        request.insert("options".into(), Value::Object(options));
    }
    if command == "relationship" {
        request.insert("edit".into(), Value::Object(edit));
    }
    parse_command(Value::Object(request)).map(|command| Invocation::Run(Box::new(command)))
}

/// `skills list | show ID | validate [FILE] | new ID | path`.
fn parse_skills(args: &[String]) -> Result<Invocation> {
    let what = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| invalid("skills requires list, show, validate, new or path"))?;
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        return Ok(Invocation::Help);
    }
    let rest = &args[1..];
    let one = |label: &str| -> Result<String> {
        match rest {
            [value] if !value.starts_with("--") => Ok(value.clone()),
            _ => Err(invalid(format!("skills {what} takes one {label}"))),
        }
    };
    let none = || {
        if rest.is_empty() {
            Ok(())
        } else {
            Err(invalid(format!("skills {what} takes no arguments")))
        }
    };
    let request = match what {
        "list" => {
            none()?;
            serde_json::json!({"command":"skills"})
        }
        "path" => {
            none()?;
            serde_json::json!({"command":"skills_path"})
        }
        "show" => serde_json::json!({"command":"skill","id":one("skill id")?}),
        "new" => serde_json::json!({"command":"skill_new","id":one("skill id")?}),
        "validate" => match rest {
            [] => serde_json::json!({"command":"skill_validate"}),
            _ => serde_json::json!({"command":"skill_validate","file":one("file")?}),
        },
        other => {
            return Err(invalid(format!(
                "Unknown skills command {other:?}; use list, show, validate, new or path"
            )))
        }
    };
    parse_command(request).map(|c| Invocation::Run(Box::new(c)))
}

/// Single edit commands: the operation each runs and its own flags.
fn edit_command(command: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match command {
        "set-activity" => ("set_activity", &["activity", "field", "value"]),
        "add-activity" => ("add_activity", &["activity", "selected", "name", "wbs"]),
        "delete-activity" => ("delete_activity", &["activity"]),
        "dissolve-activity" => ("dissolve_activity", &["activity", "retain-lag"]),
        "assign" => ("assign_resource", &["activity", "resource"]),
        "unassign" => ("remove_assignment", &["activity", "resource"]),
        "set-assignment" => (
            "set_assignment",
            &["activity", "resource", "field", "value"],
        ),
        "calendar-day" => ("calendar_day", &["calendar", "date", "shifts"]),
        "calendar-weekday" => ("calendar_weekday", &["calendar", "weekday", "shifts"]),
        "set-project" => ("set_project", &["field", "value"]),
        "add-wbs" => ("add_wbs", &["wbs", "name"]),
        "add-document" => ("add_document", &["title", "parent", "reference"]),
        "set-document" => ("set_document", &["document", "field", "value"]),
        "delete-document" => ("delete_document", &["document"]),
        "link-document" => (
            "link_document",
            &["document", "activity", "wbs", "work-product"],
        ),
        "unlink-document" => ("unlink_document", &["document", "activity", "wbs"]),
        "add-expense" => ("add_expense", &["activity", "name", "fields"]),
        "set-expense" => ("set_expense", &["expense", "field", "value"]),
        "delete-expense" => ("delete_expense", &["expense"]),
        "add-issue" => ("add_issue", &["name", "fields"]),
        "set-issue" => ("set_issue", &["issue", "field", "value"]),
        "delete-issue" => ("delete_issue", &["issue"]),
        "add-risk" => ("add_risk", &["name", "fields"]),
        "set-risk" => ("set_risk", &["risk", "field", "value"]),
        "delete-risk" => ("delete_risk", &["risk"]),
        "assign-risk" => ("assign_risk", &["risk", "activity"]),
        "unassign-risk" => ("unassign_risk", &["risk", "activity"]),
        "add-threshold" => ("add_threshold", &["parameter", "fields"]),
        "set-threshold" => ("set_threshold", &["threshold", "field", "value"]),
        "delete-threshold" => ("delete_threshold", &["threshold"]),
        "monitor-thresholds" => ("monitor_thresholds", &["from", "to", "today"]),
        "add-step" => ("add_step", &["activity", "name", "at"]),
        "set-step" => ("set_step", &["activity", "step", "field", "value"]),
        "delete-step" => ("delete_step", &["activity", "step"]),
        "move-step" => ("move_step", &["activity", "step", "to"]),
        "add-steps-from-template" => (
            "add_steps_from_template",
            &["activity", "template", "after"],
        ),
        "create-step-template" => ("create_step_template", &["activity", "name"]),
        "set-feedback" => ("set_feedback", &["activity", "notes", "review-required"]),
        "update-progress" => ("update_progress", &["data-date", "activities", "remaining"]),
        "apply-actuals" => ("apply_actuals", &["data-date", "remaining"]),
        "recalculate-costs" => ("recalculate_costs", &["active-only", "sync-overtime"]),
        "top-down-estimate" => (
            "top_down_estimate",
            &["labor-units", "wbs", "not-started-only", "weights"],
        ),
        "shift-wbs" => ("shift_wbs", &["wbs", "direction"]),
        "rename-wbs-code" => ("rename_wbs_code", &["wbs", "code"]),
        "rename-calendar" => ("rename_calendar", &["calendar", "name"]),
        "set-default-calendar" => ("set_default_calendar", &["calendar"]),
        "convert-calendar" => ("convert_calendar", &["calendar", "to"]),
        "copy-calendar" => ("copy_calendar", &["calendar", "source", "kind"]),
        "global-change" => ("global_change", &["name", "rule"]),
        _ => return None,
    })
}

/// A `--value` is the text typed, so a text field keeps it exactly (`1.10`,
/// `0042`, a 20-digit number); numeric fields read their number from it.
/// Only `null` (clear) and `true` or `false` are taken as JSON.
fn loose_value(text: &str) -> Value {
    match text {
        "null" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::from(text),
    }
}

fn shifts_value(text: &str, weekday: bool) -> Result<Value> {
    match text.trim() {
        "nonwork" => Ok(Value::Array(vec![])),
        "standard" if !weekday => Ok(Value::Null),
        other => json(other, "shifts"),
    }
}

/// `apply` and the single edit commands. They build the same requests as
/// `call` with an `apply` or `edit` command.
fn parse_edit(command: &str, args: &[String]) -> Result<Command> {
    let common = [
        "input",
        "output",
        "project",
        "workspace-id",
        "dry-run",
        "recalculate",
    ];
    let own: &[&str] = match edit_command(command) {
        Some((_, own)) => own,
        None => &["changes"],
    };
    let allowed: Vec<&str> = common.iter().chain(own).copied().collect();
    let fields = flags(args, &allowed)?;
    if fields.contains_key("project") && fields.contains_key("workspace-id") {
        return Err(invalid("Choose --project or --workspace-id, not both"));
    }
    let mut request = Map::new();
    let mut operation = Map::new();
    for (name, value) in &fields {
        match name.as_str() {
            "input" | "output" => {
                request.insert(name.clone(), Value::from(value.as_str()));
            }
            "project" => put_integer(&mut request, "project_id", value, true)?,
            "workspace-id" => put_integer(&mut request, "workspace_id", value, true)?,
            "dry-run" | "recalculate" => {
                let flag = value
                    .parse::<bool>()
                    .map_err(|_| invalid(format!("--{name} accepts true or false")))?;
                request.insert(name.replace('-', "_"), Value::Bool(flag));
            }
            "changes" => {
                request.insert("changes_file".into(), Value::from(value.as_str()));
            }
            "value" => {
                operation.insert("value".into(), loose_value(value));
            }
            "weekday" => put_integer(&mut operation, "weekday", value, false)?,
            "at" | "to" if command.ends_with("-step") => {
                put_integer(&mut operation, name, value, false)?
            }
            "labor-units" => {
                let n = value
                    .parse::<f64>()
                    .map_err(|_| invalid("--labor-units must be a number"))?;
                operation.insert("labor_units".into(), Value::from(n));
            }
            "review-required" | "active-only" | "sync-overtime" | "not-started-only"
            | "retain-lag" => {
                let flag = value
                    .parse::<bool>()
                    .map_err(|_| invalid(format!("--{name} accepts true or false")))?;
                operation.insert(name.replace('-', "_"), Value::Bool(flag));
            }
            "activities" => {
                let list: Vec<Value> = value
                    .split(',')
                    .map(|s| Value::from(s.trim()))
                    .filter(|v| v.as_str() != Some(""))
                    .collect();
                operation.insert("activities".into(), Value::Array(list));
            }
            "weights" => {
                let object = json(value, "weights")?;
                if !object.is_object() {
                    return Err(invalid("--weights must contain a JSON object"));
                }
                operation.insert("weights".into(), object);
            }
            "data-date" => {
                operation.insert("data_date".into(), Value::from(value.as_str()));
            }
            "rule" => {
                let object = json(value, "rule")?;
                if !object.is_object() {
                    return Err(invalid("--rule must contain a JSON object"));
                }
                operation.insert("rule".into(), object);
            }
            "fields" => {
                let object = json(value, "fields")?;
                if !object.is_object() {
                    return Err(invalid("--fields must contain a JSON object"));
                }
                operation.insert("fields".into(), object);
            }
            "work-product" => {
                let flag = value
                    .parse::<bool>()
                    .map_err(|_| invalid("--work-product accepts true or false"))?;
                operation.insert("work_product".into(), Value::Bool(flag));
            }
            "shifts" => {
                operation.insert(
                    "shifts".into(),
                    shifts_value(value, command == "calendar-weekday")?,
                );
            }
            _ => {
                operation.insert(name.clone(), Value::from(value.as_str()));
            }
        }
    }
    match edit_command(command) {
        Some((op, own)) => {
            for flag in own {
                let optional = (command == "add-wbs" && *flag == "name")
                    || command == "add-activity"
                    || (command == "calendar-day" && *flag == "shifts")
                    || (command == "add-document" && matches!(*flag, "parent" | "reference"))
                    || (command.ends_with("link-document")
                        && matches!(*flag, "activity" | "wbs" | "work-product"))
                    || matches!(*flag, "fields" | "from" | "to" | "today")
                    || (matches!(
                        command,
                        "add-expense" | "add-issue" | "add-risk" | "add-threshold"
                    ) && matches!(*flag, "name" | "parameter"))
                    || matches!(
                        (command, *flag),
                        ("add-step", "at")
                            | ("set-feedback", "notes" | "review-required")
                            | ("dissolve-activity", "retain-lag")
                            | ("update-progress", "activities" | "remaining")
                            | ("apply-actuals", "remaining")
                            | ("recalculate-costs", _)
                            | ("top-down-estimate", "wbs" | "not-started-only" | "weights")
                            | ("copy-calendar", "source" | "kind")
                            | ("global-change", _)
                    );
                if !optional && !fields.contains_key(*flag) {
                    return Err(invalid(format!("{command} requires --{flag}")));
                }
            }
            operation.insert("op".into(), Value::from(op));
            request.insert("command".into(), Value::from("edit"));
            request.insert("operation".into(), Value::Object(operation));
        }
        None => {
            if !fields.contains_key("changes") {
                return Err(invalid("apply requires --changes"));
            }
            request.insert("command".into(), Value::from("apply"));
        }
    }
    if !fields.contains_key("input") {
        return Err(invalid(format!("{command} requires --input")));
    }
    parse_command(Value::Object(request))
}

/// `eps list` and the EPS edits.
fn parse_eps(args: &[String]) -> Result<Command> {
    let sub = args.first().map(String::as_str).ok_or_else(|| {
        invalid("eps requires list, add-folder, rename-folder, move-folder, delete-folder, file-project or create-project")
    })?;
    let (op, own, required): (&str, &[&str], &[&str]) = match sub {
        "list" => ("", &[], &[]),
        "add-folder" => ("add_folder", &["name", "eps-id", "parent"], &["name"]),
        "rename-folder" => (
            "rename_folder",
            &["folder", "name", "new-eps-id"],
            &["folder"],
        ),
        "move-folder" => ("move_folder", &["folder", "parent"], &["folder"]),
        "delete-folder" => (
            "delete_folder",
            &["folder", "delete-projects"],
            &["folder"],
        ),
        "file-project" => (
            "file_project",
            &["workspace-id", "folder"],
            &["workspace-id", "folder"],
        ),
        "create-project" => (
            "create_project",
            &[
                "folder",
                "project-id",
                "name",
                "planned-start",
                "must-finish",
                "manager",
                "rate-type",
            ],
            &["folder", "project-id", "planned-start"],
        ),
        other => {
            return Err(invalid(format!(
                "Unknown eps command {other:?}; use list, add-folder, rename-folder, move-folder, delete-folder, file-project or create-project"
            )))
        }
    };
    let common: &[&str] = if sub == "list" {
        &["input"]
    } else {
        &["input", "output", "dry-run"]
    };
    let allowed: Vec<&str> = common.iter().chain(own).copied().collect();
    let fields = flags(&args[1..], &allowed)?;
    if !fields.contains_key("input") {
        return Err(invalid(format!("eps {sub} requires --input")));
    }
    for flag in required {
        if !fields.contains_key(*flag) {
            return Err(invalid(format!("eps {sub} requires --{flag}")));
        }
    }
    let mut request = Map::new();
    let mut operation = Map::new();
    for (name, value) in &fields {
        match name.as_str() {
            "input" | "output" => {
                request.insert(name.clone(), Value::from(value.as_str()));
            }
            "dry-run" => {
                let flag = value
                    .parse::<bool>()
                    .map_err(|_| invalid("--dry-run accepts true or false"))?;
                request.insert("dry_run".into(), Value::Bool(flag));
            }
            "workspace-id" => put_integer(&mut operation, "workspace_id", value, true)?,
            "delete-projects" => {
                let flag = value
                    .parse::<bool>()
                    .map_err(|_| invalid("--delete-projects accepts true or false"))?;
                operation.insert("delete_projects".into(), Value::Bool(flag));
            }
            _ => {
                operation.insert(name.replace('-', "_"), Value::from(value.as_str()));
            }
        }
    }
    if sub == "list" {
        request.insert("command".into(), Value::from("eps_list"));
    } else {
        operation.insert("op".into(), Value::from(op));
        request.insert("command".into(), Value::from("eps"));
        request.insert("operation".into(), Value::Object(operation));
    }
    parse_command(Value::Object(request))
}

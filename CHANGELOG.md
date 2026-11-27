# Changelog

All notable changes to OpenXER are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). The newest release is at the top.

## [0.1.2] - 2026-11-27

### Changed
- Major performance optimizations for large schedules (tested on 35,000-activity schedules) across the CPM engine, XER loader/editor, and desktop UI (pre-indexed lookups, faster calendar/WBS/grid/Gantt paths, reduced idle and scroll frame times).
- Unified desktop UI controls across dialogs, view menus, toolbars, pickers, and detail tabs onto shared `dialog_kit`, `chrome`, `details::form`, and `views::sheet` components, backed by the `no_raw_egui_controls_in_desktop_ui` regression guard.

### Fixed
- Updated EPS folder deletion and sample-project sort tests to match v0.1.1 behavior.

## [0.1.1] - 2026-11-25

### Added
- **Close Project** in the Projects view right-click menu, plus a true empty-workspace state when the last open project is closed or deleted.
- Full right-click context menu in **Assign Predecessors** and **Assign Successors**: Copy (`Ctrl+C`), Columns, Table Font and Row, Filter By, Group and Sort By, Expand All (`Ctrl+Num +`), and Collapse All (`Ctrl+Num -`).
- Expanded the default `RIVERBEND` sample project to 365 activities across six plant areas with a second baseline (Mid-August Rebaseline), Risks, Work Products & Documents, Issues, Thresholds, LOE and WBS Summary activities, WBS Milestones, OBS, Project/Resource/Assignment Codes, Funding, Budget Log, Spending Plan, Financial Periods with past period actuals, and typed UDFs.

### Fixed
- Enabled **Delete** directly on open projects (including the default `RIVERBEND` sample project) in the Projects view, saving and closing before deletion and keeping the workspace empty on restart instead of re-seeding the sample.

## [0.1.0] - 2026-11-25

The first public release.

### Added
- A native Windows desktop scheduler for XER files: open, schedule, level,
  update, check, compare, print, report and save schedules, with the
  activity table beside the Gantt chart and Activity Details underneath.
- Open any XER file, pick the projects it holds, and open several projects
  at once. Tables and fields OpenXER does not use are carried through
  untouched when the project is exported again.
- Import into a new project, replace an existing one, or add activities into
  a project you already have, with Undo Last Import. Export to XER, project
  XML or Excel (the OpenXER layout or the XER table layout, with saved export
  templates for the columns), and import from XER, project XML or Excel. The
  XML form carries projects with their schedule options, WBS, activities,
  relationships, calendars, resources with their codes, assignments,
  activity codes, user defined fields, expenses, notebooks, the OBS and
  currencies; other XER data stays in XER. XML files exported by the product
  that defines the format open in OpenXER; XML files OpenXER writes follow
  the structure of such an export but have not yet been opened in it. Reports, Schedule
  Comparison and Project Checker results can also be saved as CSV files.
- A critical path engine with calendars, shifts and holidays, every
  constraint type, relationship types and lags, Level of Effort and WBS
  summary activities, retained logic, progress override and actual dates,
  expected finish, longest path and multiple float paths. Each run writes a
  schedule log, and automatic scheduling can follow every edit.
- Resource leveling with priorities, limits, calendars, leveling within
  float and selected resources, and a leveling log of what moved.
- Progress: status, actual dates, remaining duration and percent complete in
  the table or the Status tab, Update Progress, Apply Actuals, Progress
  Spotlight, steps, suspend and resume, resource actuals and Save Period
  Actuals.
- Check Schedule scores the schedule against common health targets (open
  ends, lags, relationship types, constraints, float, long durations,
  invalid progress dates, out-of-sequence progress and more) and filters the
  table to what each check finds.
- Trace Logic, Activity Network, Activity Critical Path, driving
  relationships and Schedule Comparison between two projects or a project
  and its baseline.
- Table and Gantt layouts: columns for every schedule, resource, cost,
  earned value, code and user defined field; grouping with band totals;
  filters with nested conditions and dates relative to the data date; bars
  you can drag; curtains, sight lines, a progress line and baseline bars; a
  timescale from hours to years; and named layouts you can export and
  import.
- Bottom views: usage profiles and spreadsheets, a second table and Gantt
  chart, Trace Logic and the Activity Network, with tab groups and a second
  Activities view.
- Resources, roles, rates, shifts and calendars, all editable, with Del. /
  Merge for unused or duplicate resources.
- Resource curves: ten preset curves and the file's own. On Fixed Duration
  activities an assignment's units and costs follow its curve over its own
  dates in the usage views, time-phased reports and planned value; the
  schedule and the stored units do not change. The exact per-period numbers
  follow OpenXER's reading of the documented behavior.
- Usage profiles with stacked histograms by resource or role, actual and
  remaining in one bar, cumulative curves and each resource's limit.
- Expenses, cost accounts, funding sources, currencies, issues, risks,
  thresholds and Monitor Thresholds, including cost and earned value
  thresholds.
- Baselines: any number of them, a project baseline and up to three user
  baselines, baseline bars, variance columns and earned value against the
  baseline you choose.
- Print and Print Preview of the layout on screen, page setup, and publish
  to PDF.
- A Reports window with a starter set of scheduling reports, your own
  report definitions in groups, and batch runs to PDF, HTML, text or print
  preview.
- `openxer-cli` and a local MCP server over stdio. Every edit the app can
  make is a command and a tool that goes through the same rules as the app:
  single edits, `apply` batches that run all or nothing, dry runs that show
  every changed field, and `meta` notes on each change.
- Agents can also read and edit documents and their files, expenses,
  issues, risks, thresholds, earned value, steps, assignments, WBS, calendars
  and resource curves. `automation/COVERAGE.md` maps every app command to
  the CLI and MCP.
- The AI Agents menu connects an AI assistant on this PC to the MCP server,
  checks the connection and shows what an agent can do.
- Skills for AI agents: a library of instructions for checking and building
  schedules (logic health, critical and near-critical paths, update and
  baseline reviews, look-aheads, milestones, resources, building a schedule
  from a scope, progress updates, what-if and recovery options, time impact
  of a change and more), served to every connected agent as tools and as
  prompts. AI Agents > Skills lists them and lets you edit them or write
  your own; `openxer-cli skills` does the same from the command line.
- Read-only analysis tools agents use for every number they report: Check
  Schedule, float paths, comparison with a baseline or an earlier file,
  look-ahead, milestones, schedule statistics, resource loading and a WBS
  rollup.
- Built-in help: a topic for every dialog and main view, with contents,
  search, Back and Forward, F1 for the dialog or view in front, and
  Help > What's New... for these release notes.
- Dual license: MIT OR Apache-2.0.

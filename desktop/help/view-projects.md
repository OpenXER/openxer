# Projects

The Projects view lists every project in your workspace, filed under the folders of the EPS (enterprise project structure: the folder tree that holds your projects), with how each project and each folder is doing. Open it with `Enterprise > Projects` or the **Projects** button on the Enterprise bar.

## What it shows
- **Layout: Projects**: the strip above the table. Its menu has **Project Details** (shows or hides the bottom pane), **Group by EPS Folder** or **Group by Status**, **Show Status** check boxes that hide the projects of a status, and **Customize Columns...**.
- **Project table**: EPS folders with their projects under them. A project not yet placed sits under the first top-level folder. Each folder row shows the rollup of every project under it, child folders included.
- **Columns**: **Project ID**, **Project Name**, **Total Activities** and **Strategic Priority** at first, leaving most of the width to the chart. **Customize Columns...** lists the others by group: **Status**, **Responsible Manager** and **Project Leveling Priority**; **Start**, **Finish**, **Planned Start**, **Actual Start**, **Actual Finish**, **Anticipated Start** and **Finish**, **Data Date**, **Must Finish By**, **Late Start**, **Late Finish** and **Baseline Finish**; **Finish Variance** and **Total Float**; **Schedule % Complete (duration)**; **Budgeted**, **Actual** and **Remaining Labor Units** and **Total Cost**; **Original**, **Current** and **Proposed Budget** with the spending and benefit plan totals; the **Not Started**, **In Progress** and **Completed** activity counts; a column for each project code (the value assigned on the **Codes** tab, read here); and the user defined fields.
- **Project Gantt**: beside the table, one bar per project from its start to its finish, the part already worked (actual start to the data date, or to the actual finish) in a darker shade, a thin baseline bar under it when the project has a baseline, and a blue line at the project's data date. Folder rows get a summary bar over their projects. Drag the splitter between the table and the chart to share the width. The chart scrolls and zooms like the Activities Gantt: drag the top timescale band to pan, the lower band to zoom, or use the scroll bar, the wheel and `Ctrl`+wheel.
- **Command bar**: the buttons on the right edge: **Open Project**, **Close Project**, **Add**, **Delete**, **Cut**, **Copy** and **Paste**. **Close Project** closes the selected open project (closing the last open project leaves no project open).
- **Project Details**: tabs for the selected project: **General**, **Notebook**, **Budget Log**, **Spending Plan**, **Budget Summary**, **Dates**, **Funding**, **Codes**, **Defaults**, **Resources**, **Settings** and **Calculations**. Select an EPS folder instead and the tabs show its totals. The project **Status** (Planned, Active, Inactive, What-If, Requested) is set on the **General** tab.

## How the rollups are worked out
- **Start** is the earliest start (actual, else early) of any activity, **Finish** the latest finish (actual, else early). **Late Start** and **Late Finish** are the earliest late start and the latest late finish.
- **Data Date** is each project's own. A folder shows the latest data date of its projects. **Must Finish By**: a folder shows the latest.
- **Total Float** is the lowest total float of the activities not yet completed. A folder shows the lowest of its projects, the most critical one.
- **Schedule % Complete (duration)** is based on durations, not on costs or on how many activities are done: (planned span minus remaining span) divided by planned span. The planned span runs from the earliest start (the earliest actual start once work has begun) to the latest finish; the remaining span runs from the earliest remaining start of an unfinished activity (the data date when none is stored) to the latest remaining finish. A project with every activity completed is 100%; one not started is 0%. The spans are measured in working time of the project's default calendar. A folder uses its projects' default calendar when they all share the same one, otherwise the standard calendar new projects start with (Monday to Friday, 8 hours a day).
- **Finish Variance** is the baseline finish minus the current finish, in working time; a negative value means the project now finishes later than its baseline. The baseline is the project's project baseline, else its Primary baseline when that is a stored baseline (see Maintain Baselines). A folder compares the latest baseline finish with the latest finish of its projects that have a baseline.
- Units and costs add up. Labor units count the assignments of labor resources; costs count every resource assignment plus the project's expenses.
- The activity counts add up.

## Common tasks
1. Double-click a project, or select it and click **Open Project**, to work on it. OpenXER switches to the Activities view.
2. Select an EPS folder and click **Add** to open the New Project dialog with that folder chosen. With a project selected, its folder is chosen.
3. To move a project, select it, click **Cut**, select the target folder and click **Paste**.
4. To duplicate a project, click **Copy**, select a folder and click **Paste**. The Copy Options dialog asks what to bring along.
5. Click a column title to sort the projects inside each folder by it; click again for the reverse order, a third time to go back to Project ID order.
6. To see all jobs of one status together, choose **Group by Status** on the **Layout** menu; to leave out, say, inactive projects, clear **Inactive** under **Show Status**.

Right-click a row or a column title for the same commands (including **Open Project**, **Close Project** and **Delete**), plus **Project Details** (shows or hides the details), **Edit Project Details** (opens the **General** tab), **Columns**, **Table Font and Row...**, **Filter By**, **Group and Sort By**, **Expand All**, **Collapse All** and **Collapse To...**. Right-click the chart or its date header for **Project Details**.
- **Filter By**: **All**, **Currently Open** (the projects open now), or under **Status** one status at a time, or **Non What-if** (every project but the what-if ones).
- **Group and Sort By**: **EPS** (the folder tree), **List** (no bands), **Responsible Manager**, **Status** or **Project Leveling Priority**.
- **Table Font and Row...** sets this table's own font, cell colour and row height, with the same window as the Activities table. `View > Expand All`, `Collapse All` and `Collapse To...` work here too.

In the open project's row, click a selected cell or press `F2` to type **Project Name**, **Strategic Priority** (a whole number from 1 to 10000), **Project Leveling Priority** (1 to 100) or **Anticipated Start** or **Finish**, or to pick **Status** or **Responsible Manager**. Each change is one step for `Edit > Undo`.

## Good to know
> On the **Calculations** tab, the switch that recalculates actual units and cost from the duration percent is on in a new project. While it is on, typing a Duration % for a started activity gives every resource assignment that share of its budgeted units as actual, with the costs at its price and the rest remaining. While it is off, only the remaining duration follows the percent.
> Every value is read from the projects as stored in the workspace (the open project from its live data), so closed projects are always current. There is no separate step to summarize them first.
> The chosen columns, their widths, the sort, the grouping and the status filter are saved with the view.
> On the **Funding** tab of the open project, **Add** and **Delete** list which funding sources pay for it, with an **Amount** and **Fund Share** for each; click a cell to edit it. Each change is one undo step. If no funding sources exist yet, **Add** opens Funding Sources first.
> Only the details of the project you are working on can be edited. Other projects' tabs are shown read-only, and their **Funding** tab is empty.
> Fill Down (`Ctrl+E`, `Edit > Fill Down` or the right-click menu) works on the **Project Name** column: select two or more projects with `Shift` or `Ctrl`, and the name of the row you clicked first goes to the others as one undo step. Only the project you are working on takes a new name; the others are skipped and the status bar says so. On the other columns Fill Down is greyed.
> **Delete** works on both open and closed projects: an open project is saved and closed after confirmation before removal. Deleting the last project in the workspace leaves an empty workspace (no project open) without re-creating the sample on restart. **Delete** on a folder row deletes the folder with the folders and projects under it, after one question ([Project Folders (EPS)](project-folders)).
> Deleting the sample project RIVERBEND that a new workspace starts on also removes the calendars, resources, codes, notebook topics and other shared rows only the sample uses. A row another project uses stays, and a row of yours that was named `<name>_1` because the sample held `<name>` gets its own name back. Undo puts all of it back.
> The **Data Date** on the **Dates** tab can be typed or picked here, as in the Schedule dialog (`F9`). Any date is accepted, earlier than the current one included; nothing asks or refuses. The change is one undo step and moves no activity: schedule afterwards to recalculate the dates from it.
> On the **Settings** tab, **Define Critical Activities** decides which activities count as critical: those with total float at or below the value you type, or those on the longest path. It is the same setting as in Schedule Options; the activity table, the bars, the critical filters and reports all follow it. Each change is one undo step.
> Where a project stores no value of its own, the **Defaults**, **Resources** and **Settings** tabs show the value OpenXER uses instead (for example the Activity ID numbering a new activity gets), so no box is left blank.
> Build the folder tree itself with `Enterprise > Project Folders (EPS)...`.
> AI agents read the same rollups with `openxer-cli eps list` (see Working with AI agents).

## See also
- [Project Folders](project-folders)
- [New Project](new-project)
- [Open Project](open-project)
- [Copy Options](copy-options)
- [Funding Sources](funding-sources)
- [Maintain Baselines](maintain-baselines)
- [WBS](view-wbs)
- [Working with AI agents](working-with-ai-agents)

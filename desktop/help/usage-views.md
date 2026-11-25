# Usage Spreadsheets and Profiles

The usage views show labor and resource units period by period on the Gantt's timescale. Pick one with `View > Show on Bottom` or the Bottom Layout toolbar: **Activity Usage Spreadsheet**, **Activity Usage Profile**, **Resource Usage Spreadsheet** or **Resource Usage Profile**. The Resource Usage Spreadsheet can also fill the top pane (`View > Show on Top`).

## How units are spread over time
Each assignment's units (or costs, with **Costs** on) are spread over its own dates: its budgeted units over the planned dates, its actual units from the actual start to the actual finish or the data date, and its remaining units over the remaining dates (the leveled dates when the project is leveled) or, for remaining late units, over the late dates. An assignment with a lag starts that much after the activity, and one shorter than its activity finishes early. An assignment without units has no duration of its own, so its costs spread over the activity's dates. Units fall only in working time, on the activity's calendar, or on the resource's calendar for a resource dependent activity (also once the assignment is complete).

Without a resource curve the units spread evenly over that working time. With one, on a Fixed Duration activity, they follow the curve: see [Resource Curves](resource-curves) for the rules. The exact per-day numbers of a curve follow OpenXER's reading of the documented behavior and are not yet confirmed by stored results. An assignment with no working time at all puts its units in the period holding its start.

## Choosing the spreadsheet's fields
A usage spreadsheet starts with one value per period: remaining labor units on the Activity Usage Spreadsheet, remaining early units on the Resource Usage Spreadsheet. To show other values, right-click the spreadsheet and pick `Spreadsheet Fields...` (Activity Usage Spreadsheet) or `Spreadsheet Fields > Customize...` (Resource Usage Spreadsheet, whose submenu also offers Remaining Early Units, Actual Units and Budgeted Units to show one of them alone). The Fields dialog names each field with its measure, such as Remaining Labor Units or Cum Budgeted Cost. Move fields from **Available Options** to **Selected Options** with the arrow buttons, and order them with the up and down arrows; **Default** goes back to the single starting field. Each chosen field gets a line of its own under every activity, resource and assignment, in the order listed, and the value column names the field on each line.
- **Time Interval** fields hold each period's own value: **Planned** (spread over the planned dates), **Actual**, **Remaining**, **Remaining Late** (over the late dates) and **At Completion** (actual plus remaining).
- **Cumulative** fields hold the same values as running totals from the start of the data.
- On the Activity Usage Spreadsheet each field counts **Labor**, **Nonlabor** or **Material** resources, or **Total** for all of them. Material counts only with **Costs** on, as material units do not add to hours; Total in units adds labor and nonlabor.

**Units** or **Costs** on the Display menu decides what every field measures. The choice is saved with the layout. Show All Projects adds the other open projects only to the remaining fields.

## Typing units into periods
On the Resource Usage Spreadsheet you can set an assignment's remaining units for one period yourself. Click a period cell on an assignment's **Remaining** line, then press F2 (or double-click it) and type the units; Enter keeps them and Escape drops them. A key pressed on a selected cell opens the cell for typing without entering that key, so type the value once the cell is open.
- Only that period changes; every other period keeps its units. The assignment's remaining units become the sum of its periods, and while the activity has not started (and the project links budget to remaining) its budgeted units follow. Units/time becomes the new total over the same duration; the dates do not move. The activity's units follow its assignments.
- The assignment's **Curve** then reads **Manual**. Typing the old value back keeps it Manual. To go back to an even spread, open the Resources tab of Activity Details and clear the Curve cell (or pick another curve); OpenXER asks first, because the typed periods are deleted.
- At a week or month timescale the units you type are spread evenly over that period's working time within the assignment's remaining dates.
- Cells that take no value: resource and role lines, other projects' lines, fields other than **Remaining** (also cumulative ones), periods before the data date or outside the assignment's remaining work, and the spreadsheet in **Costs** or cumulative display.

Select several cells by Shift+clicking the last one. Right-click for the cell commands, each one undo step:
- **Fill Down** (Ctrl+E) copies the first selected line's values into the lines below it; **Fill Across** (Ctrl+H) copies each line's first selected period into the periods after it.
- **Range Copy** (Shift+Ctrl+C) keeps the selected values (and puts them on the clipboard); **Range Paste** (Shift+Ctrl+V) writes them from the first selected cell on, skipping cells that take no value.

The typed periods are saved in the XER file with the assignment, so they come back when the file is opened again. The command line and MCP set one period with `set_assignment` and the field `period_remaining_units` (`DATE=UNITS`, or `START,FINISH=UNITS` for several days).

## The resource list
The Resource Usage Profile and the Resource Usage Spreadsheet have a resource list at the left. Each pane keeps its own list choices, and they are saved with the layout.
- **Display bar**: the strip over the list reads **Display:** and the list's filter. Click it for **Filter By**, **Group and Sort By**, **Columns** and **Select View**, plus **Expand All** and **Collapse All**. Right-click the list for the same commands and the pane's own menu.
- **Filter By**: **Current Project's Resources** (the default) lists the resources assigned to an activity, or named on a risk or issue, in the open projects. **All Resources** lists every resource. **Customize...** filters by part of the Resource ID or name, by resource type, by a resource code value, and optionally to the current project's resources only.
- **Hierarchy**: parents and children show as a tree. Click a box to open or close a parent. A parent that does not match the filter still shows, greyed, so its children have context. It can be selected and shows only its own units: a parent never adds up its children.
- **Group and Sort By**: **Hierarchy**, **List** (flat), **Primary Role** or **Resource Type**, or **Customize...** to group by up to three resource codes, each in ascending or descending order. Click a column header to sort within each level; click it again to sort the other way. An arrow marks the sorted column. **Clear Sort** goes back to the resources' own order.
- **Columns > Customize...**: choose and order the list's columns, such as Resource ID, Resource Name, Resource Type, Primary Role, Calendar, Max Units/Time, Default Units/Time, Price/Unit, resource codes and resource user fields.
- **Select View**: **By Resource**, or **By Role** to list roles. With a role selected, the profile shows the role's demand: actual units, the remaining units of assignments that name a resource (staffed) and of those that name only the role (unstaffed). Roles have no limit line.

## Selecting several resources
Ctrl+click adds a resource to the selection or takes it out; Shift+click selects a range. The profile and the spreadsheet then show the sum of the selected resources: the spreadsheet has a total row, then each resource's own rows. Selecting a group row (a role, type or code group) shows the sum of the resources in it. Material units do not add to labor or nonlabor units, so a selection that mixes them shows no summed units, only a note; with **Costs** on (the Display menu) they add up.

## Display Activities for selected
Under the list are two boxes that filter the activity table in the top pane:
- **Resource**: only the activities the selected resources (or roles) work on.
- **Time Period**: click a period on the profile, or a period column in the spreadsheet, to keep only the activities working in it. The picked period shows shaded.

While either filter applies, the layout bar's **Filter:** caption says so in blue, and **Clear** at its right end shows every activity again. Both boxes are off while **Show All Projects** is on, because the activity table lists only the current project.

## Good to know
> **Show All Projects** (right-click the pane) adds the other open projects' assignments of the same Resource ID to the totals. It does not add other projects' role demand.
> The list's selection is shared by the profile and the spreadsheet; its filter, grouping, sort, columns and view are each pane's own.
> Right-click a profile for **Profile Options...**, **Stacked Histogram** and **Show Actual and Remaining**, or a spreadsheet for **Spreadsheet Options...** and **Copy to Excel**.

## See also
- [Profile Options](profile-options)
- [Resource Curves](resource-curves)
- [Activities](view-activities)
- [Resources](view-resources)
- [Roles](roles)

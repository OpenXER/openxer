# Resource Assignments

The Resource Assignments view lists every resource assignment in the open project: which resource or role works on which activity, and how much. Open it with `Project > Resource Assignments`.

## What it shows
- **Assignment table**: the assignments grouped under bands, a band for each resource unless you group them another way. Its columns come from the same list of assignment fields as the **Resources** tab of Activity Details (units, costs, prices, durations, dates, curve and more), plus the activity's ID, name, status, WBS and project.
- **Spreadsheet**: beside the table, the time-phased values of each assignment, a day per cell, the same values the Resource Usage Spreadsheet shows. It starts with the remaining units; choose other fields (budgeted, actual, remaining late, at completion, and their cumulative totals) with **Spreadsheet Fields** on its right-click menu. With several fields each assignment takes one line per field. It scrolls and zooms like the Gantt chart.
- **Assignment Details**: tabs for the selected assignment. **General** holds the resource, role, cost account, price per unit and rate type. **Planning** holds the budgeted, actual and remaining units, the durations and the dates. **User Defined Fields** and **Codes** hold the rest.

## Common tasks
1. Click the bar above the table (`Layout: ...`) for the view's own menu: **Layout**, **Assignment Details** and **Resource Usage Spreadsheet** (each shows or hides its part), **Columns**, **Group and Sort By**, **Expand All** and **Collapse All**.
2. **Columns > Customize...** picks the table's columns and their order; **Columns > Default** puts the view's own set back. Drag a column title to move it and its edge to size it.
3. **Group and Sort By** puts a band over the assignments of each resource, role, activity or cost account, or one band for the project. The assignments under a band are in Activity ID order (under an activity, in Resource ID order). Click a band's box to fold it.
4. To change a value, click a cell of the selected row, then click it again (or press F2) and type, or pick from the list. Enter or a click elsewhere keeps the value; Escape drops it. A cell takes a value when the Resources tab would take it for that activity: for example the remaining duration until the activity finishes, and the actual units once it has started. Each change can be undone on its own.
5. Select several rows and press Ctrl+E (or **Fill Down**) to copy the focused cell of the first row to the others. **Find** and **Replace** on the Edit menu work on the table's cells.
6. In the spreadsheet, click a day cell of an assignment and type to set its remaining units for that day, as in the Resource Usage Spreadsheet. **Fill Down**, **Fill Across**, **Range Copy** and **Range Paste** are on the spreadsheet's right-click menu.
7. **Add** (Insert, or the table's right-click menu) opens Assign Resources for the activity of the selected assignment or activity band. **Delete** asks first, then deletes the selected assignments; one Undo brings them back.
8. Use **Layout > Open...**, **Save** or **Save As...** to keep and reuse how the view looks: its columns and their widths, its grouping, its spreadsheet fields and what it shows. Layouts are kept in your workspace, not in the project file.

## Good to know
> The **Resource Assignments** button on the Project bar opens a separate assignments window, not this view.
> Remaining units are spread on the activity's calendar between its start and finish (the leveled dates when the schedule is leveled). For a started activity the spread begins at the data date.
> A value typed into a day cell makes the assignment's curve Manual. Taking a Manual curve off asks first, here as on the Resources tab.
> If the project has no assignments, the view says so and shows an empty table.

## See also
- [Activities](view-activities)
- [Resources](view-resources)
- [Usage views](usage-views)
- [Timescale](timescale)
- [Recalculate Assignment Costs](recalculate-assignment-costs)

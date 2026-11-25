# Resources

The Resources view lists the people, equipment and materials in the project file and lets you edit them. Open it with `Enterprise > Resources` or the **Resources** button on the Enterprise bar.

## What it shows
- **Display:** the strip above the table. Click it to choose **All Resources** or **Current Project's Resources** (only those assigned in the open project), or to **Expand All** and **Collapse All**. At its right end, **Resource Usage Profile** and **Resource Details** pick what the bottom pane shows.
- **Resource table**: a tree of resources with **Resource ID**, **Resource Name**, **Resource Type**, **Unit of Measure**, **Primary Role** and **Units/Time**.
- **Resource Details**: tabs for the selected resource: **General** (ID, name, employee ID, title, e-mail, phone, **Active**), **Codes**, **Details** (type, unit of measure, currency, overtime, calendar, default units per time), **Units & Prices** (dated rows of max units per time and price per unit), **Roles**, **Notes** and **User Defined Fields**.
- **Resource Usage Profile**: a histogram of the selected resource's actual and remaining units per period, with overallocated units shown apart. Choose **All activities** or **Selected activities**. Tick **Stacked Histogram** (or right-click the chart and choose **Stacked Histogram**) to split each bar by resource, or with **By role** by role: every resource of the selected resource's type is stacked, each in its own color, and the legend names them. Tick **Show Actual and Remaining** (also on the right-click menu) to stand actual and remaining units in one bar, actual at the bottom, instead of side by side; with **Stacked Histogram** on as well, each series' block shows its actual units at the bottom in a darker shade of its color. Both choices are saved with the layout.

## Common tasks
1. Click **Add** on the Edit bar, or right-click and choose **Add**. While **Add resources through the wizard** is on in [User Preferences](user-preferences), Assistance, the [New Resource Wizard](new-resource-wizard) opens; otherwise the new resource goes in at once. Either way it goes under the same parent as the selected one, with the project's calendar and one **Units & Prices** row, and starts as Resource ID "R" (or "R-1", "R-2" and so on when that ID is taken) named "(New Resource)".
2. Fill in its ID and name on the **General** tab, then its type and calendar on **Details**.
3. On **Units & Prices**, add a row for each date its limit or price changes.
4. To move a resource, **Cut** it, select a resource at the level where it belongs and **Paste**. It goes under the same parent as the selected resource. **Copy** and **Paste** make a duplicate with its prices, roles and codes, placed the same way.

Right-click a row or a column title for **Details** (shows or hides Resource Details), **Cut**, **Copy**, **Paste**, **Add**, **Delete** and **Delete or Merge...**, then **Columns**, **Filter By**, **Group and Sort By**, **Expand All** and **Collapse All**.

## Good to know
> A resource with activity assignments, or with child resources, cannot be deleted. Merge it into another resource with **Del. / Merge**, or remove the assignments or move the children first.
> A resource keeps at least one row on **Units & Prices**, and two rows cannot start on the same date.
> Max units and calendars here are used by resource leveling. A price is copied onto an assignment when the assignment is made; to reprice existing assignments after a price change, run Recalculate Assignment Costs.
> **Del. / Merge** deletes the selected resource or merges it, and any duplicates you tick, into another resource: assignments, rates, roles, codes and field values move, with a preview first. See [Delete or Merge Resources](merge-resources).

## See also
- [Delete or Merge Resources](merge-resources)
- [Roles](roles)
- [Resource Shifts](resource-shifts)
- [Level Resources](level-resources)
- [Recalculate Assignment Costs](recalculate-assignment-costs)
- [Resource Assignments](view-assignments)

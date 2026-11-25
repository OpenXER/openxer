# Earned Hours

Shows the labor hours earned so far and writes them to an Excel workbook: for each resource, how many budgeted hours the work done to the data date has earned. Open it with `Tools > Reports > Earned Hours...`, or run **Earned Hours** from the **Scheduling Reports** group of the Reports view (double-click it, or select it and click **Run Report**).

The figures are cumulative to the data date. To get one period's earned hours, export after each update and subtract last update's file from this one in Excel: the rows are sorted by ID and the columns keep their order, so the two files line up row for row.

## What is on it
- **Break down by**: the rows of the first sheet. Resource (the default), WBS element or activity.
- **OK**: opens the report in a report tab.
- **Cancel**: closes the dialog without running the report.
- **Help**: opens this topic.

## The report tab
- **Earned to date** and **By activity**: the same rows and columns as the workbook's two sheets, with the total row.
- **Export > Excel (.xlsx)...** asks where to save and writes the workbook below. The other Export formats, **Print** and **Copy** work as for every report tab.
- **Refresh** works the report out again after you change the project. **Options...** opens the dialog again with the tab's choice; **OK** shows the new report in the same tab.

## The workbook
- **Earned to date**: one row per resource (or WBS element, or activity) with Budgeted Units, Earned Units, Actual Units, Remaining Units, At Completion Units and % Earned, and a total row.
- **By activity**: one row per labor assignment, by Activity ID and then Resource ID, with the activity name, WBS, budgeted hours, the percent complete used, earned, actual and remaining hours.
- **Info**: the project, data date, where the budget comes from and which percent complete rule was used.

## How earned hours are worked out
- Earned hours are the budgeted hours times the activity's performance percent complete, the same figure as the Earned Value columns in labor units.
- The budgeted hours come from the assigned project baseline (or the primary baseline when the project uses it). Without a baseline the current budget is used, and the Info sheet says so.
- The performance percent complete follows each WBS element's Earned Value tab: by default the activity percent complete (physical, duration or units, as the activity is set).

## Good to know
> A project without labor hours gets a message and no report.
> Earned Hours is built in: in the Reports view it runs, and it cannot be modified, exported as a definition, copied, deleted or added to a batch.
> The same workbook comes from the command line with `openxer-cli export-earned-hours`.

## See also
- [S-Curve](s-curve)
- [Assign Baselines](assign-baselines)
- [WBS view](view-wbs)

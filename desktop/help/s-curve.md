# S-Curve

Shows the project's S-curve on screen and writes it to an Excel workbook: hours (or cost, or days) per period, the running totals, and a chart, ready to paste into an update report. Open it with `Tools > Reports > S-Curve...`, or run **S-Curve** from the **Scheduling Reports** group of the Reports view (double-click it, or select it and click **Run Report**). The choices are filled in from what the project holds, so pressing **OK** right away gives a correct report.

## What is on it
- **Measure**: what the curve counts. Only what the project has is offered. Labor units (hours) come first when labor assignments have budgeted hours, then cost (assignments and expenses), then duration. The line under it says what was found, for example "Labor hours found on 412 of 430 activities".
- **Period**: day, week, month, or the project's financial periods when it has them. Week is the default; weeks start on the timescale's first day of the week.
- **Break down by**: total only, resource (the default when the measure has resources), or WBS level 1 or 2. Each row of the breakdown sheets is one resource or WBS element.
- **Add the late dates curve**: adds actual plus remaining spread over the late dates, next to the early forecast.
- **Only activities in the current filter**: limits the curve to the activities the current filter shows in the active project. Off by default: every activity of the open projects counts.
- **OK**: opens the report in a report tab.
- **Cancel**: closes the dialog without running the report.
- **Help**: opens this topic.

## The report tab
- The chart: the period values as bars (budgeted, and actual with remaining on top), the cumulative budgeted, actual, and actual plus remaining as lines on the right scale, the late dates line when it was asked for, the earned value to the data date as a diamond, and the data date as a blue line. The timescale under it shows the periods.
- Under the chart: one row per period with the same columns as the workbook, a total row, and with a breakdown the totals of each resource or WBS element and their values by period.
- **Export > Excel (.xlsx)...** asks where to save and writes the workbook below. The other Export formats, **Print** and **Copy** work as for every report tab; the PDF and the printout include the chart.
- **Refresh** works the report out again after you change the project. **Options...** opens the dialog again with the tab's choices; **OK** shows the new report in the same tab.

## The workbook
- **S-Curve**: one row per period with its start and end, the period's budgeted, actual and remaining values, the cumulative columns and the cumulative percent of the budget, and the earned value to the data date in the data date's row. The chart shows the period values as columns and the cumulative budgeted, actual, and actual plus remaining as lines on the right axis, with earned as one marker. The cumulative columns are formulas, so a number you change in Excel carries into the totals and the chart.
- **By Resource - Budgeted**, **- Actual**, **- Remaining** (or by WBS): one row per resource or WBS element and one column per period, with a total row and a total column.
- **Info**: the project, data date, measure, period, where the budget comes from, when the file was made, and any warnings.

## How the numbers are worked out
- Budgeted comes from the assigned project baseline (or the primary baseline when the project uses it), spread over the baseline's own planned dates. Without a baseline it is the current plan. The Info sheet says which.
- Actual runs up to the data date. Remaining starts at the data date and follows the early dates (the leveled dates when the project levels).
- Hours and costs are spread the same way the usage profiles and spreadsheets spread them, resource curves included, so a week here holds the same hours as that week in the Activity Usage Profile.
- Duration: each activity counts its planned (original) duration in working days on its own calendar, spread evenly over its planned working days. Actual is that number times the activity's percent complete; remaining is the rest. Milestones count nothing. The cumulative percent columns show the curve as a share of the total.
- Earned to date is the earned value of every activity in the measure: the baseline budget times the performance percent complete set on each WBS element's Earned Value tab.

## Good to know
> The same workbook comes from the command line with `openxer-cli export-scurve`.
> S-Curve is built in: in the Reports view it runs, and it cannot be modified, exported as a definition, copied, deleted or added to a batch.
> Warnings on the Info sheet point out activities without labor assignments, budgets without dates and assignments that follow a resource curve.

## See also
- [Earned Hours](earned-hours)
- [Usage profiles and spreadsheets](usage-views)
- [Assign Baselines](assign-baselines)

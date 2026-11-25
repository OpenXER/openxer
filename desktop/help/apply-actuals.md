# Apply Actuals

Moves the data date and records progress, as scheduled, for the work set to compute its actuals automatically. The data date is the date progress is measured up to. Open it with `Tools > Apply Actuals...`.

## What is on it
- **Projects to update**: a list of the open projects with **Project ID**, **Project Name**, **Current Data Date**, **New Data Date** and **Project Planned Start**. Click a **New Data Date** cell to type in it.
- **A new date for each project**: type each project's date in its **New Data Date** cell in the list. This is the default, and each date starts at that project's current data date. A project left at its current date is not changed.
- **One new date for every project**: type one date in the **New Data Date** box below; it is used for every open project.
- **How to set remaining durations**: **Always recalculate** sets the remaining duration to the working time from the new data date to the scheduled finish. **From its duration type** does that for fixed duration activities, and for fixed units and fixed units/time activities takes the elapsed working time off the remaining duration instead. The dialog opens with the choice you used last time (**Always recalculate** at first).
- **Apply**: applies the actuals and closes the dialog. `Enter` does the same. A short notice confirms it; `Enter` or **OK** closes it.
- **Cancel**: closes without changes.

## What it records
Auto Compute Actuals can be set on an activity (the check box column **Auto Compute Actuals** under General in Columns), on a resource and on an expense.
- **Activity set**, or **every assignment's resource set**: the activity progresses as scheduled. One that should have started before the new date gets an actual start on its scheduled start (never before the current data date); one that should have finished by it gets an actual finish and becomes complete; one still running gets a new remaining duration by the rule you picked. All its assignments get actuals as below.
- **Only some assignments' resources set**: just those assignments get actuals. The activity's dates, status and remaining duration stay as they are.
- **Assignment actuals**: the actual units grow by the assignment's budgeted units per hour times the working time from its remaining start to the new data date, never more than its remaining units. The remaining units drop by the same amount, actual and remaining costs follow at the assignment's price, and actual this period grows with them when the project links actual to date and actual this period.
- **Expense set** (spread evenly over its activity): once its activity has started, its actual cost is the part of its budget that matches the share of the activity's original duration worked up to the new data date, the rest remaining. The activity itself is not changed by it.
- Nothing set: nothing is progressed; the data date still moves.

The new date is also saved on each project as its apply actuals date.

## Common tasks
1. Choose `Tools > Apply Actuals...`.
2. Click each project's **New Data Date** cell and type its new date.
3. Pick the remaining duration rule.
4. Click **Apply**, then press `F9` to reschedule.

## Good to know
> A new data date later than today asks you to confirm first, since it is often a typing slip. **Yes** is the default.
> Any new data date is accepted. A project left at its current date is not changed. An earlier date moves that project's data date back (and is saved as its apply actuals date) without adding or removing progress: actual dates, remaining durations, assignment actuals and expenses already recorded stay as they are.
> Running it again later adds the next period's actuals to what is there.
> Timesheet actuals are not part of XER files, so they are not applied.
> Apply Actuals does not reschedule. Press `F9` afterwards.
> All open projects change together in one undo step.

## See also
- [Update Progress](update-progress)
- [Schedule](schedule)
- [Save Period Actuals](save-period-actuals)

# Update Progress

Records progress as if the work went exactly as scheduled up to a new data date, then moves the data date there. The data date is the date progress is measured up to. Open it with `Tools > Update Progress...`.

## What is on it
- **Current Data Date**: the project's data date now.
- **New Data Date**: the date to update to. Any date is accepted: later, earlier or the same as the current data date (see "Moving the data date back" below). It starts at the current data date, also with Progress Spotlight on; after you drag the spotlight's end it starts at that end.
- **With the spotlight on, update**: **All highlighted activities** updates the activities the spotlight highlights, or every activity when the spotlight is off. **Selected activities only** updates every selected activity of the active project (all the rows of a multi-selection); it is greyed out when no activity is selected.
- **How to set remaining durations**: **Always recalculate** sets the remaining duration to the working time from the new data date to the scheduled finish. **From its duration type** does that for fixed duration activities, and for fixed units and fixed units/time activities takes the elapsed working time off the remaining duration instead. The dialog opens with the choice you used last time (**Always recalculate** at first).
- **Apply**: updates progress and closes the dialog. `Enter` does the same. A short notice confirms the update; `Enter` or **OK** closes it.
- **Cancel**: closes without changes.

## What it records
- An activity scheduled to start before the new data date gets an actual start on its scheduled start, never before the current data date.
- An activity scheduled to finish by the new data date gets an actual finish on its scheduled finish and becomes complete.
- An activity still running at the new data date gets a new remaining duration by the rule you picked.
- The scheduled dates used are the early dates, or the leveled dates when resources are leveled.
- A milestone due by the new data date is marked complete.
- Activities whose percent complete type is physical get a percent that matches the duration done, unless the project takes physical percent from steps.
- An expense set to compute actuals, spread evenly over its activity, gets the part of its budget that matches the share of the activity's original duration worked so far; a completed activity's expense is spent in full.

## Moving the data date back
The data date can be whatever you set it to.
- **The same date**: the update runs and records nothing new; no activity is progressed past the current data date.
- **An earlier date**: the data date moves back to it and the notice shows as usual. No progress is added and none is taken away: actual dates, remaining durations, assignment actuals and expenses already recorded stay as they are. To undo progress, change those values yourself or use `Edit > Undo`.

## Common tasks
1. Choose `Tools > Update Progress...`.
2. Type the **New Data Date**, for example the end of this week.
3. Click **Apply**.
4. Correct any activity that did not go to plan, then press `F9`.

## Good to know
> Update Progress does not reschedule. Press `F9` afterwards, as the status bar reminds you.
> Completed activities, level of effort, WBS summary and hammock activities are left alone.
> Resource assignment units and costs are not changed by Update Progress; Apply Actuals advances them for what is set to compute actuals.
> The data date moves even when no activity changes.
> With several projects open, every open project is updated in one undo step.
> It assumes the plan happened. Check the results against what the field reports.

## See also
- [Apply Actuals](apply-actuals)
- [Schedule](schedule)
- [Activities view](view-activities)

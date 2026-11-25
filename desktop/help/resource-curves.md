# Resource Curves

A resource curve describes how an assignment's units are meant to be spread over its duration, for example heavy at the start or bell-shaped instead of even. Open this dialog with `Enterprise > Resource Curves...`.

Curves are stored with the assignment, saved, and exported in the XER file. A curve shapes how the assignment's units and costs are spread over time in the usage spreadsheets and profiles, the histograms, time-phased reports and planned value. It never changes the schedule: dates, float, durations, stored units and leveling stay the same.

## What is on it
- **Resource Curve Name**: the curve's name. Click it on the selected row to rename it.
- **Default**: ticked for curves the file marks as default curves. These cannot be renamed, changed or deleted, only viewed.
- **Preset**: ticked for OpenXER's preset curves (below). These cannot be renamed, changed or deleted either.
- **Shape of the selected curve**: a small bar chart of the selected curve, so you can see its shape before opening or picking it.
- **Close**: closes the dialog.
- **Add**: adds "New Curve", copied from the selected curve (or an even spread when there is no curve to select). The copy can be renamed and changed.
- **Delete**: removes the selected curve, after asking. Disabled for default and preset curves.
- **Modify...**: opens the selected curve in the curve editor. Double-clicking a row does the same.
- **Help**: opens this topic.

## Preset curves
Every project offers these curves, even when its file has none:

- **Even**: The same amount in every 5% step.
- **Early Weighted**: Heaviest at the start and falling away; about seven eighths of the units in the first half.
- **Late Weighted**: Light at the start and heaviest at the finish.
- **Ramp Up**: Rising in a straight line from nothing to the most at the finish.
- **Ramp Down**: Falling in a straight line from the most at the start to nothing at the finish.
- **Bell, Centered**: Slow start, peak at mid duration, slow finish.
- **Bell, Early Peak**: A bell that peaks at 30% of the duration.
- **Bell, Late Peak**: A bell that peaks at 70% of the duration.
- **Two Peaks**: Two equal peaks at 25% and 75% of the duration with a dip between them.
- **Steady with Ramps**: Ramps up over the first 20%, holds level, and ramps down over the last 20%.

A preset is written into the project file only when an assignment uses it, so a file you send elsewhere carries only the curves it needs. Once written it stays a preset after you save and reopen, and it is never added twice. If the file already has its own curve with a preset's name, the file's curve is the one shown and used. To change a preset's shape, select it, click **Add**, and edit the copy.

## The curve editor
The window is titled **Edit Resource Curve**, or **View Resource Curve** for a default or preset curve.
- **Resource Curve Name**: the name, editable.
- A bar chart of the 21 points, one for each 5% step of the duration from 0 to 100.
- A row of 21 boxes under the chart: the percent of the units spent in each step. Type a number from 0 to 100 in each; a blank box counts as 0.
- **OK**: checks the values, scales them so they total 100%, and saves. Disabled for default and preset curves.
- **Cancel**: closes without saving. **Help**: opens this topic.

## How a curve shapes the spread
- **Which activities**: a curve shapes the spread only on activities whose duration type is **Fixed Duration and Units/Time** or **Fixed Duration and Units**. On **Fixed Units** and **Fixed Units/Time** activities the curve stays stored but is ignored, and the units spread evenly. The Curve picker says so when you hover it on such an activity.
- **Over what time**: the curve runs from the assignment's own start (after its lag) to its own finish, on the calendar the assignment works on. Non-working time carries nothing.
- **The 21 values**: each value from 5% to 100% is the share of the units spent in that twentieth of the assignment's working time; within a twentieth the units spread evenly. The 0% value is used only for actuals: it is spent at the actual start.
- **Budgeted and remaining units** follow the curve, and so do their costs. **Planned value** follows the curved budget. **Expenses** are not affected.
- **In progress**: actual units follow the curve from the actual start to the data date. The remaining units follow the rest of the curve from how far through its working time the assignment is, scaled so they add up to the remaining units. Remaining late units use the same part of the curve over the late dates.
- **Not changed**: Units/Time stays the average over the whole assignment. Leveling always spreads evenly. A curve moves units between periods but does not change an assignment's totals: when the periods shown cover the assignment's dates, they add up to its stored units.
- **When the periods do not add up to the stored units**: periods that cover only part of the assignment's dates show only that part. Some units have no dates to spread over and appear in no period: actual units on an activity without an actual start, remaining units on a completed activity, and budgeted units on an activity without planned dates. The CLI `usage` command reports such units as `unplaced`.
- **No curve**: the units spread evenly over the assignment's working time.

> The rules above follow the documented behavior of resource curves. Where that documentation is silent (how the values map onto working time, the 0% value, in-progress assignments), the exact per-day numbers follow OpenXER's own reading and are not yet confirmed by stored results.

## Common tasks
1. Select a curve close to the shape you want and click **Add**.
2. Rename the copy.
3. Click **Modify...**, type the values in the 21 boxes, and watch the bars.
4. Click **OK**.

To give an assignment a curve, pick it in the **Curve** column of the activity's Resources tab. Hovering over a curve in that list shows its shape.

## Good to know
> The values do not have to total 100: OK scales them for you. At least one value must be above zero.
> A curve you add cannot take a preset's name.
> Deleting a curve that assignments use leaves those assignments with no curve.
> Escape closes the curve editor first, then the Resource Curves dialog.

## See also
- [Assignments view](view-assignments)
- [Usage Spreadsheets and Profiles](usage-views)
- [Spread Budget Hours](spread-budget-hours)

# Spread Budget Hours (Top-Down)

Spreads a total of labor units (budget hours) down the WBS to the activities, by estimation weights. Use it when you know the total hours for the job or an area but not yet for each activity. Open it with `Tools > Spread Budget Hours (Top-Down)...`.

## What is on it
- **Estimate labor units for**: the whole project or one WBS element. Only that part of the WBS receives hours. It starts on the element selected in the WBS view, or on the whole project.
- **Total labor units**: the number of hours to spread. It must be above zero.
- **Not started activities only**: when ticked, only activities that have not started receive hours. It starts ticked.
- **The WBS list**: **WBS Code**, **WBS Name**, **Estimation Weight** and the **Labor Units** each element would get. The list updates as you type.
- **Estimation Weight**: the weight of the WBS element selected in the list. Type a new value, zero or more, to change it.
- **The summary line**: how many units go to how many activities, or what is missing.
- **Apply**: saves the weights and writes the hours to the activities.
- **Cancel**: closes without changes. `Escape` does the same.

## How the split works
At each level, the units are shared among the child WBS elements and the element's own activities in proportion to their weights. A child element with no activity that can receive hours gets no share. A higher weight gets a bigger share. An activity uses its own estimation weight, and a blank weight counts as 1. When all the weights at a level are zero, the share is equal.

## Common tasks
1. Choose `Tools > Spread Budget Hours (Top-Down)...`.
2. Pick the WBS element in **Estimate labor units for**.
3. Type the total in **Total labor units**.
4. Select each WBS row and adjust its **Estimation Weight** until the **Labor Units** column looks right.
5. Click **Apply**.

## Good to know
> Only Task Dependent and Resource Dependent activities receive hours. Milestones, level of effort and WBS summary activities do not.
> An activity's hours are split over its labor assignments in proportion to their budgets (equally when those are zero), and their costs follow; with no labor assignment they go on the activity itself. Remaining units become the new budget less the actuals, never below zero.
> Durations and dates are not changed.
> **Apply** is one undo step and is greyed out when no activity can receive hours.

## See also
- [WBS view](view-wbs)
- [Assignments view](view-assignments)
- [Activities view](view-activities)

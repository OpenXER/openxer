# Thresholds

The Thresholds view lists the open project's thresholds. A threshold watches one value, such as total float, across the activities or WBS elements of a WBS branch, and raises an issue for each one that falls below the lower limit or rises above the upper limit. Open it with `Project > Thresholds`.

## What it shows
- **Display:** the strip above the table. Click it to show **All Thresholds**, **Enabled Thresholds** or **Disabled Thresholds**, to **Expand All** or **Collapse All**, and to show or hide the details pane with **Details**.
- **Threshold table**: one row per threshold with **Threshold Parameter**, **Lower Threshold**, **Upper Threshold**, **Detail To Monitor**, **WBS**, **Responsible Manager**, **Priority**, **Status**, **From Date** and **To Date**.
- **Command bar**: **Add**, **Delete** and **Monitor** on the right edge.
- **Threshold Details**: two tabs for the selected threshold.
- **General**: the **Threshold Parameter**, the lower and upper limits, **Detail to Monitor** (Activities or WBS), **Status** (Enabled or Disabled), the **WBS to Monitor** (pick it with **WBS...**) and the **Monitor Time Window** dates.
- **Details**: the **Responsible Manager** and **Issue Priority** given to the issues it raises, the **Project Issue Count**, and a **Threshold Issues** list of those issues. **Go To** opens the selected issue in the Issues view; **Delete** deletes it.

## Common tasks
1. Click **Add** (or press `Insert`). A new threshold starts enabled, watching **Total Float (days)** on activities with a lower limit of 0, so it catches activities with negative float (float: how many days an activity can slip before it delays the project).
2. Change the parameter or the limits if you want something else.
3. Pick the WBS branch and, if you want, a time window. With no WBS picked, the whole project is watched.
4. Click **Monitor** to run it. This opens Monitor Thresholds, which checks every enabled threshold and raises the issues.

Right-click a row for **Add**, **Delete**, **Threshold Details** and **Hide Detail Window**.

## Good to know
> **Customize Columns...** on the right-click menu picks and orders the columns; the choice is kept with the view layout.
> Monitoring checks total float, free float, and start and finish date variance (the difference from the baseline dates, or from the project's planned dates when there is no baseline), all in days. Completed activities are left out of the float checks, and free float cannot be watched at the WBS level.
> The duration and cost percents and the earned value parameters (AV, CV, SV, VAC, CPI, CVI, SPI, SVI) are checked too, from the same figures the earned value columns show; see [Monitor Thresholds](monitor-thresholds).
> Running Monitor again updates the open issue a threshold raised before instead of adding a second one.
> Deleting a threshold keeps the issues it raised; they just lose their link to it.

## See also
- [Monitor Thresholds](monitor-thresholds)
- [Issues](view-issues)
- [Issue Navigator](issue-navigator)

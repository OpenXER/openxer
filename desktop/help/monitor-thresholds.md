# Monitor Thresholds

A threshold watches one measure, such as total float or start variance, and flags any activity or WBS element whose value falls below the lower limit or above the upper limit. Monitoring checks every enabled threshold and raises an issue for each item out of range. Open it with `Tools > Monitor Thresholds...`, or with the Monitor button in the Thresholds view.

## What is on it
- **Use original threshold monitor windows**: each threshold uses its own From Date and To Date, as set in the Thresholds view.
- **Use new threshold monitor window**: every threshold uses the dates you type here instead.
- **From Date** and **To Date**: the window for the second choice. They are greyed out unless it is picked.
- **Monitor**: runs the check, then switches to the Issues view with all issues shown.
- **Cancel**: closes without running anything.
- **Help**: opens this topic.

## Common tasks
1. Set up your thresholds in the Thresholds view: the parameter, the lower and upper limits, the WBS branch and the dates.
2. Reschedule with `F9` so float and dates are current.
3. Open `Tools > Monitor Thresholds...`, pick a window choice and click **Monitor**.
4. Review the new issues in the Issues view.

## Good to know
> Running the monitor again updates the open issue it raised before for the same item, instead of adding a second one.
> Float and the start and finish variances are checked in days of each activity's calendar. Variances compare against the project baseline, or the project's own planned dates when it has none.
> Every parameter is checked. **Duration % of Original** is actual duration over original duration, and **Cost % of Budget** is actual cost over budget at completion. The earned value amounts are computed as in the earned value columns, with each WBS element's earned value techniques: AV = planned value - actual cost, CV = earned value - actual cost, SV = earned value - planned value, VAC = budget at completion - estimate at completion; CPI = EV / AC, SPI = EV / PV, CVI = CV / EV and SVI = SV / PV.
> An index whose divisor is zero (an activity with no actual cost or no planned value yet) has no value and raises nothing. At WBS level an element adds up its activities' amounts and takes the indexes from the sums. Values are rounded to two decimals.
> If a date cannot be read, the dialog says so and nothing runs.

## See also
- [Thresholds view](view-thresholds)
- [Issues view](view-issues)
- [Issue Navigator](issue-navigator)

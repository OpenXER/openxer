# Resource Shifts

A resource shift splits each day into periods, for example a day shift and a night shift, so a resource can have a different limit in each. Open this dialog with `Enterprise > Resource Shifts...`.

## What is on it
- **Shift Name** list: the shifts. Click the name of the selected shift to rename it in place.
- **Shift Periods** list: the periods of the selected shift, in order of start time.
- **Start Time**: the whole hour the period starts, from 0 to 23. Click it on the selected row to change it.
- **Hours**: worked out for you, from this start to the next period's start (the last period runs round to the first).
- **Period**: the period's number, Shift 1, Shift 2 and so on.
- **Close**: closes the dialog.
- **Add**: adds "New Shift" with one period starting at 00:00.
- **Delete**: removes the selected shift and its periods, after asking.
- **Add Period**: adds a period at the next free hour after the last start.
- **Delete Period**: removes the selected period. It stays disabled while the shift has only one period.
- **Help**: opens this topic.

## Common tasks
1. Click **Add**, then click the new shift's name and type a name for it.
2. Click **Add Period** for each extra period.
3. Click each period's **Start Time** and type the hour it begins, such as 7 or 19.
4. Check the **Hours** column adds up the way you expect.

## Good to know
> Two periods of the same shift cannot start at the same hour, and a shift has at most 24 periods.
> Deleting a shift leaves the resources that used it with no shift. Their limit rows for its periods are kept but no longer tied to a shift period.
> Deleting a period works the same way: resource limit rows set for that period are kept without a shift period.

## See also
- [Resources view](view-resources)
- [Level Resources](level-resources)

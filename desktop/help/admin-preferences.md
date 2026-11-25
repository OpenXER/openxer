# Admin Preferences

Admin Preferences shows the settings that apply to the whole workspace rather than to one user. Most of them are shown for reference only, because XER files carry no administrative settings. Open it with `Admin > Admin Preferences...`.

## What is on it
- **General** tab, read only:
- **Code Separator**: the character placed between code levels, such as between the levels of a WBS code. It shows the open project's own separator.
- **Starting Day of Week**, **Date Filters**, **Activity Duration** and **Password Policy** are shown but not available.
- **Time Periods** tab, read only:
- **Hours per Time Period**: the default work hours in a day, week, month and year (8, 40, 172 and 2000). **Use assigned calendar to specify the number of work hours for each time period** is ticked: durations are converted with the hours of each activity's calendar, and these defaults are used only where a calendar gives none.
- **Time Period Abbreviations**: the letters shown after durations and units, such as h for hours and d for days.
- **Earned Value** tab:
- The two technique sections (performance percent complete, and Estimate to Complete) are shown but not available.
- **Earned value calculation**: this one can be changed. **When calculating earned value from a baseline use** picks **Budgeted values with planned dates** or **Budgeted values with current dates**. Earned value is a way to measure progress in money or hours: the budget of the work done so far. This choice sets which baseline dates that budget is spread over.
- **Help**: opens this topic.
- **Close**: closes the dialog.

## Common tasks
1. Open `Admin > Admin Preferences...`.
2. Click the **Earned Value** tab.
3. Pick planned or current dates under **Earned value calculation**.
4. Click **Close**. The change takes effect at once.

## Good to know
> Hover over a read-only area to see why it cannot be changed.
> The earned value choice is kept in your workspace and applies to every project you open.
> To change how dates, units and costs look to you, use [User Preferences](user-preferences) instead.

## See also
- [User Preferences](user-preferences)
- [Maintain Baselines](maintain-baselines)
- [Currencies](currencies)

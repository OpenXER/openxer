# Calendar Editor

Edits one calendar: its normal week, the holidays and other exception days, and the hours per day, week, month and year. Open it from [Calendars](calendars) (`Enterprise > Calendars...`) with **Modify...** or by double-clicking a calendar. The title shows the kind and name, for example "Global Calendar: Standard 5 Day".

## What is on it
- **Total work hours/day**: you set each day as a number of hours.
- **Detailed work hours/day**: you set each day half hour by half hour.
- **The month view**: one month of days. It opens on the data date's month with the data date selected. **<** and **>** change month. Click a day to select it, `Shift`+click to select a range, `Ctrl`+click to add or remove one day. Right-click for **Nonwork** or **Normal**.
- **Work hours/day**: in total mode, the hours of the selected days, 0 to 24 in half hour steps. In detailed mode, a grid of the day's half hours; click one to switch it between work and nonwork on every selected day.
- **Standard**, **Nonwork**, **Exception**: the legend. An exception is a day that differs from its normal weekday, such as a holiday.
- **Inherit holidays and exceptions from Global Calendar**: the global calendar this one copies exceptions from. It is shown, not changed, here.
- **Work**: makes the selected nonwork days working days, with their weekday's hours or 8 hours.
- **Nonwork**: makes the selected days nonwork days.
- **Standard**: returns the selected days to their normal weekday hours.
- **Workweek...**: opens [Calendar Weekly Hours](calendar-weekly-hours) for the normal week.
- **Time Periods**: sets **Hours/Day** (up to 24), **Hours/Week** (up to 168), **Hours/Month** (up to 744) and **Hours/Year** (up to 8784), used when durations are shown in days, weeks, months or years. Its **OK** needs **Hours/Day** above 0.
- **OK**: saves the calendar. **Cancel** drops every change.

## Common tasks
1. Click **>** until the month of a holiday shows.
2. Click the holiday and click **Nonwork**.
3. Repeat for each holiday, then click **OK** and press `F9`.

## Good to know
> Nothing is saved until **OK**.
> When you click **OK**, changes to this calendar's exceptions are copied into the calendars that inherit from it, on each date where they still matched it.
> A day set back to its weekday hours stops being an exception.

## See also
- [Calendars](calendars)
- [Calendar Weekly Hours](calendar-weekly-hours)
- [Scheduling basics](scheduling-basics)

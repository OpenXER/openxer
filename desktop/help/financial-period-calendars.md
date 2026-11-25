# Financial Period Calendars

Defines financial period calendars: named sets of reporting periods, such as months or weeks, that period actuals are stored in. Open it with `Admin > Financial Period Calendars...`.

## What is on it
- **Financial Period Calendars**: the list of calendars. **Calendar Name** can be edited in place. Tick **Default** to make a calendar the default; projects without their own calendar use it.
- **Start Date** and **End Date** (in the **Batch Create** box): the span to fill with periods. They start at the first day of the data date's month and the day before one year later.
- **Month ending on day**: monthly periods that end on the day you pick, 1 to 30 or **Last day**.
- **Week ending on**: weekly periods that end on the weekday you pick.
- **Batch Create**: creates back-to-back periods for the selected calendar, the first starting on **Start Date** and the last ending on or after **End Date** (at most 1,200). Monthly periods are named by month and year, weekly ones by their last day.
- **Financial Periods**: the periods of the selected calendar, with **Period Name** (editable), **Start Date** and **End Date**.
- **Add**: adds "New Financial Period Calendar". The first calendar becomes the default. You can have at most 10.
- **Delete**: deletes the selected calendar and its periods. Projects that used it fall back to the default calendar.
- **Delete Period**: deletes the selected period.
- **Close**: closes the dialog.

## Common tasks
1. Click **Add**, then click the new calendar's name and type a name for it.
2. Set **Start Date** and **End Date** in **Batch Create**.
3. Choose **Month ending on day** and **Last day** for calendar months.
4. Click **Batch Create** and check the list of periods.
5. Tick **Default** if every project should use it.

## Good to know
> New periods may not overlap periods the calendar already has. Delete the periods in that range first.
> Deleting a calendar or a period that holds stored period actuals deletes that stored performance too. OpenXER asks before it does.
> Each change is one undo step.

## See also
- [Save Period Actuals](save-period-actuals)
- [User Preferences](user-preferences)
- [Admin Preferences](admin-preferences)

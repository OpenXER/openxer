# Save Period Actuals

Stores the progress made in one financial period, so you can later report actual units and costs period by period. A financial period is a reporting slice of time, such as a week or a month. Open it with `Tools > Save Period Actuals...`.

## What is on it
- **Projects to be updated**: how many projects are open. **Store Now** stores into every one of them. You cannot type here.
- **Financial Period**: the period to store into, shown with its start and end dates. The list holds the periods of the project's financial period calendar, or of the default calendar when the project has none. It opens on the period you last chose in this session, else on the period that holds the data date, else on the last period that ends before it.
- **Store Now**: stores the period actuals and closes the dialog.
- **Cancel**: closes without storing.

## What it stores
For each activity, OpenXER stores what was earned since the periods that end before the chosen one: labor and nonlabor units, and labor, nonlabor, material and expense costs. Each assignment's actual units and cost are stored the same way. Activities with nothing new get no row. The stored periods then add up to the actuals to date. Rows already stored in the chosen period are replaced. After the store, the "this period" actual values of the activities and assignments start again from zero.

## Common tasks
1. Enter progress for the period and press `F9`.
2. Choose `Tools > Save Period Actuals...`.
3. Check that **Financial Period** shows the period that just ended.
4. Click **Store Now**. The status bar says how many activities were stored.

## Good to know
> The list is empty until financial periods exist. Create them in `Admin > Financial Period Calendars...`.
> Only the periods that overlap the range set in User Preferences are listed. If none fall in it, widen that range.
> **Store Now** is greyed out while the list is empty. Point at the empty list to see why.
> Storing is one undo step, across every open project.
> Each other open project stores into the period of its own financial period calendar that has the same start and end as the chosen one. A project without such a period is left alone, and the status bar names it.
> Store once per period, at the end of it. Storing again into the same period replaces what was there.

## See also
- [Financial Period Calendars](financial-period-calendars)
- [User Preferences](user-preferences)
- [Apply Actuals](apply-actuals)

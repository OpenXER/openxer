# Calendars

Lists the work calendars and lets you add, rename, edit and delete them. A calendar says which days and hours are working time; activities are only scheduled in working time. Open it with `Enterprise > Calendars...`.

## What is on it
- **Global**, **Resource**, **Project**: which calendars to list. Global calendars can be used by any project, resource calendars by resources, and project calendars belong to the open project.
- **The calendar list**: **Calendar Name**, plus **Default** for global calendars (tick it to make that the default global calendar) or **Personal** for resource calendars. Click a selected name again, or press `F2`, to rename it. Double-click to edit it.
- **Add**: opens **Select Calendar To Copy From**, which lists the global calendars (plus the resource or project calendars when that list is shown) with the default global calendar picked. **Select** adds the copy as "New Calendar", ready to rename.
- **Delete**: deletes the selected calendar after you confirm in **Delete Calendar**. When activities, projects, resources or other calendars use it, **Calendars in Use** opens instead (see below).
- **Modify...**: opens the [calendar editor](calendar-editor).
- **Used By...**: opens **Calendar Used By**.
- **To Global**: turns the selected project calendar into a global one.
- **To Shared** and **To Personal**: switch a resource calendar between shared and personal.
- **Select**: shown only when the dialog was opened to pick a resource's calendar. It sets the selected calendar on the resource and closes the dialog.
- **Close**: closes the dialog.

## Calendar Used By
Shows where the selected calendar is used. Choose **Projects** or **Resources** to list the projects or resources that use it. With **Projects** chosen, a line also counts the activities using it. **Close** closes it.

## Calendars in Use
Opens when you delete a calendar that something uses. Pick what takes its place, then click **OK**:
- **Select Replacement Calendar** opens **Select Replacement Calendar**, a list of the global calendars. Pick one and click **Select**.
- **Use Default Global Calendar** uses the default global calendar.

Every activity, project, resource and calendar that used the deleted calendar then uses the replacement, and the calendar is deleted. It is one step: `Edit > Undo` brings the calendar back and puts every user back on it. **Cancel** leaves everything as it was.

## Common tasks
1. Choose `Enterprise > Calendars...` and pick **Global**.
2. Click **Add**.
3. In **Select Calendar To Copy From**, pick the calendar closest to what you need and click **Select**.
4. Type a name and press `Enter`, then click **Modify...** to set its hours and holidays.

## Good to know
> The default global calendar cannot be deleted. Make another global calendar the default first.
> The command line and AI agents delete a calendar in use only when told which calendar takes its place.
> Names must be unique within each list (ignoring case) and cannot be blank.
> Project and resource calendars copied from a global calendar inherit its holidays and exceptions.
> Calendar changes do not move dates until you press `F9`.

## See also
- [Calendar editor](calendar-editor)
- [Calendar Weekly Hours](calendar-weekly-hours)
- [Resource Shifts](resource-shifts)
- [Scheduling basics](scheduling-basics)

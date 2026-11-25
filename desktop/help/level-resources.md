# Level Resources

Delays activities so their resources are not booked beyond their limits. Open it with `Tools > Level Resources...` or press `Shift+F9`.

## What is on it
- **Automatically level resources when scheduling**: every `F9` run levels too. It is the same setting as **Level resources during scheduling** in Schedule Options.
- **Consider assignments in other projects with priority equal/higher than**: not supported by the leveler. The box can only be unticked: if a file has it on, **Level** stops with a message until you untick it.
- **Preserve scheduled early and late dates**: always on. Leveling places the work on its remaining dates and keeps the early and late dates and float of the plain schedule. If a file has it off, tick it to level.
- **Recalculate assignment costs after leveling**: reprices assignments after a leveling run. This box is kept in the workspace, not in the XER.
- **Level all resources**: levels every resource. Untick it and click **Select Resources...** to tick only the resources to level; the others are left unlimited.
- **Level resources only within activity Total Float**: an activity is delayed only while it keeps at least the float typed in **Preserve minimum float when leveling**. If it cannot, it stays where its logic puts it and the resource stays over-allocated there.
- **Max percent to over-allocate resources**: not applied yet. You can enter 0 to 1000 and it is saved with the project and written to the XER, but leveling ignores it: every resource is leveled to its plain limit whatever the percent.
- **Leveling priorities**: the order activities get resources first. Each row is a **Field Name** (Activity Leveling Priority, Total Float, Early Start, Late Start, Remaining Duration, WBS or an activity code) and a **Sort Order**. **Add**, **Remove**, **Move Up** and **Move Down** edit the list.
- **Use scheduling options from**: shows the open project. **Change** is not available.
- **Log to file**, **As Html** and the path: the same log settings as the Schedule dialog.
- **Level**: saves the settings and runs the schedule with leveling.
- **Close**: saves the settings without leveling. `Escape` or the close box drops the changes.
- **View Log**: opens the Schedule Log. **Default** puts back the settings of a new project; it leaves **Automatically level resources when scheduling** as it is.

## Common tasks
1. Press `Shift+F9`.
2. Set the priorities, for example Total Float ascending so the least float goes first.
3. Click **Level**. The status bar says how many activities were delayed.

## Good to know
> Leveling never splits an activity: each one is moved as a whole.
> With automatic leveling off, the leveled dates stay on screen until the next `F9`, which drops them.
> Activities left over-allocated because of the float limit are listed in the log.
> If a setting cannot be leveled, nothing changes and a message names it.

## See also
- [Schedule Options](schedule-options)
- [Schedule](schedule)
- [Recalculate Assignment Costs](recalculate-assignment-costs)

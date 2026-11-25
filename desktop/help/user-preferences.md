# User Preferences

User Preferences sets how units, durations, dates and costs are shown to you, and how some edits behave. Open it with `Edit > User Preferences...`. Changes apply at once and are remembered for next time.

## What is on it
- **Time Units** tab:
- **Units Format** and **Durations Format**: the **Unit of Time** (Hour, Day, Week, Month or Year), a **Sub-unit** box to add the next smaller unit, the number of **Decimals**, and whether to show the unit label (not available while **Sub-unit** is on). **Example** shows the result. A new workspace shows whole days and hours without a label.
- **Units/Time Format**: show resource units per time as a percentage (50%) or as units/duration (4h/d).
- **Dates** tab: **Date Format** (month, day, year order), **Time** (12 hour, 24 hour or no time, plus **Show minutes**), **Options** (**4-digit year**, **Month name**, **Leading zeroes**, **Separator**) and a **Sample**. A new workspace shows dates without a time.
- **Currency** tab: the currency costs are shown in, chosen from the project's currencies, with **Show currency symbol** and **Show decimal digits**.
- **Assistance** tab: **Wizards**, **Add resources through the wizard** and **Add activities through the wizard** (both on unless you turn them off): Add in the Resources view opens the [New Resource Wizard](new-resource-wizard), and Add in the Activities view the [New Activity Wizard](new-activity-wizard). Off, Add puts the new row in at once. Ticking **Skip this wizard next time** in a wizard clears its box. **Out of Sequence**, **Warn before progress goes out of sequence** (off unless you tick it). On, starting or finishing an activity, typing its actual dates, or adding or changing a relationship that leaves an activity with progress ahead of an unfinished predecessor asks first: **Yes** keeps the change, **No** takes it back, **View** lists the predecessors that are not finished. Off, nothing asks. Either way the schedule log lists such activities under its warnings, and `View > Check Schedule > Out of Sequence Activities` shows only them.
- **Application** tab:
- **Application Startup Window**: the view that opens first: Activities, WBS, Projects or Resources.
- **Group and Sorting**: show the ID / Code and the Name / Description on grouping bands.
- **Codes**: show a code's **Code Value** or its **Description** in code columns.
- **Financial Period Data**: two dates and **Select** limit which financial periods are loaded. Leave both empty to load them all. A line says how many are loaded.
- **Dialogs**: **Show Copy Activity Options when pasting activities** brings back the [Copy Activity Options](copy-options) window after **Skip this dialog next time** was ticked there. **Ask whether a moved Start should hold the activity with a constraint** brings back that question, asked for a typed Start and for a dropped bar (see [Activities](view-activities)), after **Always use this answer** was ticked on it.
- **Recovery**: **Keep a recovery copy of unsaved changes every** _n_ **minutes** (on, every 5 minutes, unless you change it). Clear the box to turn it off. Whole minutes from 1 to 120.
- **Calculations** tab:
- **Resource Assignments**: when you add or remove assignments, either keep the existing assignments' units, duration and units/time, or recalculate them from the activity's duration type.
- **Assignment Staffing**: when you swap the resource on an assignment, use the new resource's units per time and overtime factor, keep the current ones, or ask each time. Also whose price per unit wins, the resource's or the role's, or ask.
- **Dissolving Activities**: **Retain Lag** (off unless you tick it) decides the lag of the relationships `Edit > Dissolve` creates. Off, they get no lag. On, each one gets the lag of the predecessor relationship plus the lag of the successor relationship it replaces; the dissolved activity's duration is not added.

## Common tasks
1. On **Dates**, pick Day, Month, Year and turn on **Month name** for dates like 07-Nov-26.
2. On **Time Units**, set durations to Day with no decimals.
3. Click **Close**.

## Good to know
> These settings are yours, not the project's. The Time Units, Dates, Currency and Application settings change how values look, not the values stored or exported. The Calculations settings change what later edits do.
> For the Financial Period Data dates, enter both or neither.
> A recovery copy is kept apart from your saved projects in the workspace and is deleted when you save. If OpenXER closes before you save, the next start asks whether to **Recover** the changes (they come back as unsaved changes; save to keep them) or **Discard** them and keep the saved projects. The question shows when each copy was kept. Escape leaves the question for the next start, as long as the copy is still newer than the saved project: once that project is saved again (here, by an agent or by an import), its older copy is deleted and never offered.

## See also
- [Admin Preferences](admin-preferences)
- [New Activity Wizard](new-activity-wizard)
- [New Resource Wizard](new-resource-wizard)
- [Currencies](currencies)

# Update Baseline

Brings chosen data from the open project into an existing baseline, without making a new one. Use it, for example, to add newly approved scope to the baseline while keeping its original dates. Open it by selecting a baseline in [Maintain Baselines](maintain-baselines) and clicking **Update...**. A baseline in use (assigned as project, primary, secondary or tertiary baseline) can be updated too.

## What is on it
The list at the left picks the page of options. Your choices are kept for the next time you open the window.

- **General**: the project-level data. **Project details** (on at first) carries the project's planned start, must finish by date, anticipated start, priorities, web address and folder; **Project UDFs** go with it. **Risks, issues, thresholds** carries the project's risks and issues (matched by name) and its thresholds, with **Risk UDFs** and **Issue UDFs**. **Work products, documents** carries the documents (matched by name) and their links to the updated activities, with their UDFs. **WBS UDFs** carries the UDF values of the WBS elements.
- **Compare all records** and **Run optimized** are shown greyed: OpenXER always compares all the data in full, so the first one is shown ticked, and the second has no meaning here.
- **Log to file**: writes a log of the update (the options and every change) to the file named below it. Click **...** to choose the file.
- **Activities**: **All Activities**, or **Only activities in filter:** with the filters ticked in **Activity Filters**, opened with **...**: tick one or more standard or saved filters and choose **All selected filters** (an activity must pass every one) or **Any selected filters** (passing one is enough). The box shows the filters chosen. Then what happens to activities: **Add activities missing from the baseline (no actuals)** adds the project activities the baseline lacks, with their relationships, codes, notebooks, steps, expenses and UDFs, but without actual dates, units or costs. **Remove baseline activities deleted from the project** removes them; the filter does not limit this. **Update matching activities** (on at first) updates the activities both have, by the Activity page.
- **Activity**: what an update of an existing activity carries. **General activity information** (name, Activity ID, type, duration type, percent complete type, calendar, primary resource and the like), **Relationships**, **Constraints**, **Dates, durations, data date** (all dates, durations, float and status; without a filter the baseline's data date follows too), **WBS assignments**, **Risk assignments**, **Activity UDFs**, **Activity code assignments**, **Activity notebooks**, **Steps** with **Step UDFs**, **Expenses** with **Expense UDFs**. Activities with no assignments: **Budgeted units, cost** and **Actual units, cost** of the activity itself.
- **Resource Assignment**: **Update matching assignments**, each with or without **Budgeted units, cost** and **Actual units, cost**; **Add missing assignments** (on at first), the same two choices for what the new ones bring; **Assignment UDFs** and **Codes**.
- **Default**: puts every option back to how it starts.
- **Preview**: opens **Preview Changes**, the list of what **Update** would do with these options. The tree at the left groups the changes under **Project**, **WBS** and **Activities**, one line per changed activity marked **Added**, **Deleted** or **Updated**. Select a line to see its fields at the right, with the project's value and the baseline's value.
- **Update**: updates the baseline at once and tells you when it is done. With **Log to file** on, **View Log** shows the log.
- **Close**: closes the window. When you changed options, you are asked whether to keep them: **Yes** keeps them for the next time, **No** forgets them, **Cancel** goes back.

## Common tasks
1. Choose `Project > Maintain Baselines...` and select the baseline.
2. Click **Update...**.
3. To add new scope only, open **Activities**, tick **Add activities missing from the baseline (no actuals)** and untick **Update matching activities**.
4. Click **Preview** to check the list, then **Update**.

## Good to know
> An option you leave off leaves that data in the baseline exactly as it was.
> Ticking **Dates, durations, data date** overwrites the baseline dates of the updated activities, which removes the slip you were measuring.
> A project activity and a baseline activity are the same activity when they carry the same internal identity, so an activity renumbered after the baseline was taken is updated, not deleted and added again. Activities from a file without that identity are matched by Activity ID.
> WBS elements, project calendars and project activity codes that the carried data needs are added to the baseline.
> The baseline's **Last Update Date** is set to now. The update is one undo step.
> The same update is available to AI agents as `openxer-cli baseline update` and the MCP tool `openxer_baseline_update`; a dry run returns the preview.

## See also
- [Maintain Baselines](maintain-baselines)
- [Assign Baselines](assign-baselines)
- [Filters](filters)

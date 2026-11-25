# Project Checker Results

This window shows what the Project Checker found. It opens after you click **Generate Report** in the [Project Checker](project-checker).

## What is on it
- A summary line at the top: how many checks ran and how many found problems, counting the passed checks even when **Report Errors Only** leaves them out of the list. When a report file is written, here or by an Export button, a note says where.
- Results list, one row per check (and per project when several were checked):
- **Check**: what was looked for.
- **Severity**: Error or Warning.
- **Checked**: how many records the check looked at.
- **Found**: how many problems it found.
- **Result**: Passed (green), or Found (red for errors, yellow for warnings).
- **Project**: which project, when more than one was checked.
- A line of advice under the list for the selected check: what the problem means and how to fix it.
- Findings list for the selected check: **Activity ID**, **Activity Name** and **Detail** of each problem.
- **Close**: closes the window.
- **Go To**: switches to the Activities view and selects the highlighted finding's activity. Double-clicking a finding does the same. It is enabled only for findings in the open project.
- **Export HTML...**, **Export Excel...**, **Export PDF...**, **Export CSV...**: save the results as a report file of that type, with a save box.
- **Help**: opens this topic.

## Common tasks
1. Click a check whose **Result** is Found.
2. Read the advice line under the list.
3. Double-click a finding to jump to the activity and fix it.
4. Run the checker again to confirm the fix.

## Good to know
> When Report Errors Only was ticked, checks that passed are left out of the list, and the summary line says so.
> A relationship finding is listed under its successor activity, with the relationship in **Detail**.
> The results do not refresh as you edit. Run the checker again to see the effect of your fixes.

## See also
- [Project Checker](project-checker)
- [Activities view](view-activities)

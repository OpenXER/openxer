# Project Checker

The Project Checker runs a list of quality checks on one or more projects: open ends, constraints, lags, float, missing records, duplicate IDs and more. It then shows the results and can save them as a report. Open it with `Tools > Project Checker...`.

## What is on it
- **Select Projects**: **Selected Projects** lists what will be checked; the open project is there to start. **Available Projects** lists the other projects in your workspace. Use **< Add to list** and **Remove from list >** to move the highlighted project across. Projects other than the open one are checked as last saved in the workspace.
- **Options**:
- **Business Rule Checks**, **Foreign Key Checks**, **Unique Key Checks**: turn a whole family of checks on or off. Business rules look at the logic and dates; foreign key checks find records that point at something missing; unique key checks find duplicates.
- **Run Priority Checks Only**: runs only the ticked checks that are marked High.
- **Report Errors Only**: leaves passed checks out of the results and the report.
- **Group By Options**: **Check** or **Activity**, how the saved report is laid out.
- **Lag limit (days)**, **Duration limit (days)**, **Float limit (days)**: the limits used by the checks for long lags, long remaining durations and large float (float: how many days an activity can slip before it delays the finish). A value counts only when it is longer than the limit. They start at 5, 44 and 44 days, and must be numbers of 0 or more.
- Check list: every check with its **Severity** (Error or Warning) and **Priority** (High or blank). Click a check box to turn one check on or off. A check that will not run says (off).
- **Report**: **Save Report To** names the report file; it starts as ProjectCheckerReport.html in your Documents folder, and the **...** button picks another place. The file type follows the extension (.html, .xlsx, .pdf or .csv). Leave it blank to skip the file. **Overwrite Existing** (on to start) lets it replace a file already there; when it is off and the file exists, nothing runs and the dialog says so.
- **Generate Report**: runs the checks, writes the file if one is named, closes this dialog and opens [Project Checker Results](project-checker-results). It is disabled while **Selected Projects** is empty.
- **Close** (or `Escape`), **Help**.

## Common tasks
1. Add any other projects you want to check.
2. Turn the check families and single checks on or off, and set the limits.
3. Fill in **Save Report To**, or clear it for results on screen only.
4. Click **Generate Report**.

## Good to know
> When you click **Generate Report**, the checks, options, limits and report path are saved in your workspace for next time. The project list and **Overwrite Existing** start fresh each time.
> The checks read the project as it is. Reschedule with `F9` first so dates and float are current.

## See also
- [Project Checker Results](project-checker-results)
- [Check Schedule](check-schedule)

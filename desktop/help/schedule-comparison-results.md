# Schedule Comparison Results

Shows every difference a [Schedule Comparison](schedule-comparison) found, one row per change, with filters to narrow the list. It opens when you click **Compare** in the Schedule Comparison dialog (`Tools > Schedule Comparison...`).

## What is on it
- **The header**: which two projects were compared, the project finish of each and the difference between them, and counts of activities and relationships added, deleted and changed.
- **Show**: **All Changes**, **Added**, **Deleted**, **Changed**, **Activities**, **Relationships** or **Project and WBS**.
- **Area**: **All Areas** or one kind of activity field, such as **Dates** or **Float**. Picking one kind shows only the field changes of that kind, so added and deleted rows, relationships, project and WBS rows are hidden.
- **Find**: type part of an Activity ID, activity name or field name to shorten the list.
- **The change list**: **Change** (added, deleted or changed), **Activity ID**, **Activity Name**, **Field**, **Original**, **Revised** and **Variance**.
- **Go To**: switches to the Activities view and selects the activity of the chosen row in the open project. For a relationship row it selects the successor. It is greyed out for project and WBS rows and for deleted activities.
- **Export HTML...**, **Export Excel...**, **Export PDF...** and **Export CSV...**: save the report in that format.
- **Close**: closes the window.

## Reading the variance
**Variance** is the revised value minus the original: calendar days for dates, days of the revised project's working day length for durations and float, and points for percents. A positive date variance means the revised date is later. Other fields have no variance.

## Common tasks
1. Set **Show** to **Changed** and **Area** to **Dates** to see what slipped.
2. Type an Activity ID in **Find** to see everything that changed on one activity.
3. Double-click a row, or select it and click **Go To**, to look at the activity in the activity table.
4. Click **Export Excel...** to send the list to someone.

## Good to know
> **Go To** works only for activities that exist in the open project. Otherwise the status bar says the activity is not in the open project.
> The filters change only what you see. The exports write the full report.
> When the dialog wrote an **Output File**, or after an export, a note gives the name of the file saved.

## See also
- [Schedule Comparison](schedule-comparison)
- [Activities view](view-activities)
- [Maintain Baselines](maintain-baselines)

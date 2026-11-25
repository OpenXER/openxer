# Schedule Comparison

Compares two versions of a schedule and lists what was added, deleted and changed between them. Use it to see what moved between updates, or how the current schedule differs from a baseline. Open it with `Tools > Schedule Comparison...`.

## What is on it
- **Original Project or Baseline**: the older version. The list holds **<Current Project>**, every saved workspace project, the stored baselines of every workspace project, and any project you read from an XER file with the **...** button beside it. It starts on the open project's first baseline, or on the next entry in the list when the open project has none.
- **Revised Project**: the newer version, picked the same way. It starts on **<Current Project>**, the open project as it is now, saved or not.
- **Select information to compare**: **Project Information**, **Added/deleted WBS**, **Added/deleted Activities** and **Added/deleted Relationships**.
- **Activity Information**: the activity fields to compare for activities found in both versions: general information (name, type, calendar, WBS), original and remaining duration, start and finish, status and actual dates, primary and secondary constraints, activity percent complete, total and free float, and activity code assignments.
- **Group report by activity**: in the report file, lists each changed activity with its changes under it. Untick it to get one table per area instead. It is ticked to start with.
- **Send Report To**: **HTML file**, **Excel file**, **PDF file** or **CSV file**. Tick **Output File** and give a path to also write the report to that file. Picking a format changes the file's extension to match. The path starts as `ScheduleComparison.html` in your Documents folder.
- **Compare**: runs the comparison and opens [Schedule Comparison Results](schedule-comparison-results).
- **Cancel**: closes without comparing.

## Common tasks
1. Choose `Tools > Schedule Comparison...`.
2. Set **Original Project or Baseline** to last period's update, or click **...** to read it from an XER file.
3. Leave **Revised Project** on **<Current Project>**.
4. Untick any area you do not need.
5. Click **Compare** and read the results.

## Good to know
> Activities are matched by Activity ID, WBS elements by WBS code and relationships by their predecessor and successor Activity IDs. A renumbered activity shows as one deleted and one added. A WBS element that kept its code but was renamed shows as changed.
> The two boxes must name different entries in the list, or the dialog says "Pick two different projects to compare."
> When you click **Compare**, your choice of areas, grouping, report format and output file is remembered for next time.
> Comparing changes neither project.

## See also
- [Schedule Comparison Results](schedule-comparison-results)
- [Maintain Baselines](maintain-baselines)
- [Open Project](open-project)

# Check Schedule

Tests the schedule against fifteen common quality checks and lists the activities and relationships that fail them. Open it with `Tools > Check Schedule...` or `View > Check Schedule > Check Schedule...`.

## What is on it
- **Use Check Schedule Parameters from**: the open project, the only choice.
- **The check list**: one check box per check. Tick the ones to run. **Long Lags**, **Large Float** and **Large Durations** take a limit in hours (**h**).
- **Target**: the pass mark for each check, as a percent of the activities or relationships checked (0 to 100). Most checks pass below the target (**<**). **Relationship Types** (share of finish to start links) and **BEI** pass above it (**>**). The BEI target is an index, not a percent (0.95 by default). A blank target gives a result with no pass or fail; **Long Lags** starts blank.
- **Save Report To**: an HTML file for the report. It starts as `ScheduleCheckReport.html` in your Documents folder; clear the box to skip the file. **Overwrite Existing** (on at first) lets the report replace a file of the same name.
- **Check Schedule**: runs the ticked checks with the settings shown and opens the results. It does not keep the settings for next time.
- **Save**: keeps the ticked checks, hour limits and targets for the rest of the session without running.
- **Default**: puts back the standard checks, limits (352 hours) and targets.
- **Cancel**: closes without running.

## Check Schedule Results
A window with one row per check: **Check**, **Count** (items that failed), **Of** (items checked), **Result** and **Target**. A result in green passed, red failed. Click a check to list its activities or relationships below; click one to jump to that activity (for a relationship, its successor). If the network has loops, a **Circular Relationships** list shows them. When a report file was written, the window says where.

## Common tasks
1. Choose `Tools > Check Schedule...`.
2. Untick checks you do not need and adjust the targets.
3. Check the path in **Save Report To**, or clear it if you do not want a file.
4. Click **Check Schedule**, then work through the failing rows in the results.

## Good to know
> Most activity checks look only at work that is not complete, and skip level of effort and WBS summary activities. **Large Durations**, **Resources / Cost** and **Late Activities** also skip milestones. **Activities with Invalid progress dates** and **Out Of Sequence** look at every activity.
> Hard constraints are Start On, Start On or Before, Finish On, Finish On or Before and the mandatory ones. Soft constraints are Start On or After, Finish On or After and As Late As Possible.
> **Late Activities** and **BEI** (Baseline Execution Index: activities finished against those the baseline planned to finish by the data date) use the project baseline chosen in [Assign Baselines](assign-baselines). When none is assigned, or an activity is not in it, the activity's own planned finish is used.
> If a report file already exists and **Overwrite Existing** is off, the report is not saved and the status bar says so.

## See also
- [Schedule](schedule)
- [Assign Baselines](assign-baselines)
- [Project Checker](project-checker)

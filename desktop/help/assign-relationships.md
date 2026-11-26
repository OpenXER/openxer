# Assign Predecessors and Successors

Picks activities to link to the selected activity. A predecessor is an activity that must happen first; a successor is one that follows. Open it with `Edit > Assign > Predecessors...` or `Edit > Assign > Successors...`, the same items on the activity table's right-click menu, or the **Assign** button on the Predecessors or Successors tab of the activity details. The window is titled **Assign Predecessors** or **Assign Successors**.

## What is on it
- **Find**: type part of an activity ID or name to shorten the list.
- **The activity list**: **Activity ID** and **Activity Name** of every other activity, grouped under WBS bands. With several projects open it also shows **Project ID** and lists the activities of every open project. Right-click the list for **Copy** (`Ctrl+C`), **Columns**, **Table Font and Row...**, **Filter By**, **Group and Sort By**, **Expand All** (`Ctrl+Num +`) and **Collapse All** (`Ctrl+Num -`).
- **Relationship Type**: **FS** (finish to start, the usual type), **SS** (start to start), **FF** (finish to finish) or **SF** (start to finish). New links start as FS.
- **Lag**: a wait added to the link, typed like a duration. It starts at 0.
- **Assign**: adds a relationship to each activity selected in the list. Click one activity, `Ctrl`+click to add or drop more, or `Shift`+click to select a run of them.
- **Close**: closes the window. **Help**: opens this topic.

## Common tasks
1. Select the activity in the activity table.
2. Choose `Edit > Assign > Predecessors...`.
3. Type part of the predecessor's ID in **Find** and click it in the list.
4. Set the **Relationship Type** and **Lag** if it is not a plain FS with no lag.
5. Click **Assign**, or double-click the row. Repeat for more predecessors, then click **Close**.

## Good to know
> The window stays open after each **Assign**, so you can add several links in a row.
> With more than one activity selected, the window says "Assigns to the N selected activities." Each one gets the same link.
> A link to an activity in another open project is added to one activity at a time, with one activity picked in the list.
> If a new link puts an activity that already has progress out of sequence and the out-of-sequence warning is on (User Preferences, Assistance tab), OpenXER asks whether to keep it. When one **Assign** links several selected activities, every new link is checked and you are asked about each out-of-sequence one in turn.
> Each click of **Assign** is one undo step, even when it links several selected activities. Press `F9` afterwards to see the new dates.

## See also
- [Edit Relationship](edit-relationship)
- [Activities view](view-activities)
- [Scheduling basics](scheduling-basics)

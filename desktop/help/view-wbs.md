# WBS

The WBS view shows the project's work breakdown structure: the tree of elements that the activities are filed under. Open it with `Project > WBS` or the **WBS** button on the Project bar.

## What it shows
- **Layout: WBS**: the strip above the table. Click it for **Expand All**, **Collapse All** and a **WBS Details** check box that shows or hides the bottom pane.
- **WBS table**: one row per element with **WBS Code**, **WBS Name** and **Total Activities** (the activities under it and under the elements below it) at first. The top row is the project itself. Click the arrow beside a row to fold or unfold its children.
- **Columns**: **Columns > Customize...** lists every column by group: **General** (status, responsible manager, estimated weight), **Dates** (start, finish, actual and anticipated dates, remaining early dates), **Durations** (total float), **Budget** (original, current and proposed budget, spending and benefit plan totals, independent estimates to complete), **Number of Activities**, **Percent Completes**, **Units**, **Costs**, **Earned Value** and **User Defined**. Columns summed from the activities give the same figure as a WBS band of the Activities table grouped by WBS: the element's own activities and those of the elements below it.
- **Summary chart**: beside the table, a bar per element spanning the dates of the activities under it. It scrolls and zooms like the Gantt chart.
- **Command bar**: the buttons on the right edge: **Add**, **Delete or Merge...**, **Cut**, **Copy**, **Paste**, **Shift up**, **Shift down**, **Shift left** and **Shift right**.
- **WBS Details**: tabs for the selected element: **General** (code, name, status, responsible manager, anticipated dates), **Notebook**, **Budget Log**, **Spending Plan**, **Budget Summary**, **WBS Milestones**, **Documents** and **Earned Value**.

## Common tasks
1. Select the element the new one should follow and click **Add**. The new element goes just after it, at the same level. With the project row selected, it goes last at the top level.
2. Type the code and name on the **General** tab.
3. **Shift left** moves an element out one level, to just after its parent. **Shift right** puts it last under the element above it. **Shift up** and **Shift down** reorder it among its siblings.
4. To move a branch, select it, click **Cut**, select the element it should go under and click **Paste**. It goes in as that element's last child.
5. To copy a branch, select it, click **Copy**, select the element it should go under and click **Paste**. **Copy WBS Options** opens (see below).

## Copy WBS Options
Opens when you paste copied elements. Under **Optionally Include**, tick what comes along with the elements (all are ticked at first):
- **Notebook**: their notebook topics.
- **WBS Milestones**: their milestones.
- **WPs & Docs**: the work products and documents assigned to the elements themselves.
- **Activities**: the activities filed under the copied element and the elements below it. **Copy Activity Options** then chooses what comes with them (see [Copy Activity Options](copy-options)). Each copy is filed under the copy of its own element. **Renumber Activity IDs** follows to give the copies their IDs, as when pasting activities in the Activities table (see [Renumber Activity IDs](renumber-activity-ids)). When that window is turned off, a copy keeps its Activity ID when no activity in the project has it; otherwise it takes the next ID from the project's numbering.

**OK** pastes; the whole paste, elements and activities, is one step for `Edit > Undo`. **Cancel** in any of these windows pastes nothing. The choices are kept for the next paste. With **Skip this dialog next time** ticked, later pastes in this session skip the window and use the choices you made.

Right-click a row or a column title for the same commands, plus **WBS Details** (shows or hides the details), **Columns**, **Table Font and Row...**, **Group and Sort By**, **Expand All**, **Collapse All** and **Collapse To...**. Right-click the chart or its date header for **WBS Details**.

## Editing in the table
Click a selected row's cell, or press `F2`, to edit **WBS Code**, **WBS Name** and **Anticipated Start** or **Finish** in place, and to pick **Status** or **Responsible Manager** from a list. The code cell shows the whole path; type the element's own code (the parent's path in front of it may stay). Each change is one step for `Edit > Undo`. Columns summed from the activities are not typed in.

## Group, font and levels
- **Group and Sort By**: **Default** shows the tree, **Responsible Manager** lists the elements under a band for each manager, **List** lists every element once without the tree.
- **Table Font and Row...** opens the same window as in the Activities table; the font, cell colour and row height you set there apply to the WBS table only and are kept with the view.
- **Collapse To...** folds the tree to the WBS level you pick. `View > Expand All`, `Collapse All` and `Collapse To...` work here too.

## Good to know
> **Delete or Merge...** always asks first. The **Delete** key and **Edit > Delete** do the same on the selected element. On an element that holds activities or child elements, **Merge Element(s)** moves them to the parent and removes the element; **Delete All** removes the element, everything below it and those activities.
> The project row cannot be cut, copied or deleted, and an element cannot be pasted inside itself.
> A WBS code must be unique among its siblings.
> The **Earned Value** tab sets how the element measures performance percent complete (**Activity percent complete**, **WBS Milestones percent complete**, **0/100**, **50/50** or **Custom percent complete** with its percent) and how the estimate to complete is computed (remaining cost, or PF x (budget at completion - earned value) with PF = 1, 1 / CPI, 1 / (CPI x SPI) or a factor you type). The earned value columns and Monitor Thresholds use it. Each change is one undo step and is saved with the WBS in XER. Elements without their own settings follow the project row. **Use resource curves / future period buckets** stays greyed out: planned value always follows an assignment's resource curve where one applies (see [Resource Curves](resource-curves)), and future period buckets are not modeled.

## See also
- [Activities](view-activities)
- [Projects](view-projects)
- [Responsible Managers](responsible-managers)

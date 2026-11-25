# Filters

Chooses which activities the table and Gantt chart show. Choose `View > Filter By > Filters...`, or right-click the table and choose **Filters...**. The `View > Filter By` menu also lets you pick **All Activities**, or one built-in filter to use on its own, without opening the window.

## What is on it
- **All Activities**: shows everything and ignores the filters ticked below. Ticking a filter clears it.
- **Show activities that match**: **All selected filters** (an activity must pass every ticked filter) or **Any selected filter** (passing one is enough).
- **Filter** and **Select**: the list of filters. Tick **Select** to use a filter. The **Default** group holds the built-in filters, such as Critical, Completed, In Progress, Not Started, Milestone, Negative Float and Longest Path. The **Global** group is empty. The **User Defined** group holds the filters you make.
- **When applying the selected filter(s)**: **Replace activities shown in current layout** hides the activities that do not match. **Highlight activities in current layout which match criteria** keeps every activity and marks the matching ones.
- **OK**: applies and closes. **Apply** applies and stays open. **Cancel** closes and keeps the old filters.
- **New...**: opens the [Filter editor](filter-editor) on a new filter named "(Untitled Filter)".
- **Delete**, **Copy** and **Modify...**: act on the user filter picked in the list. Copy makes "Copy of" the filter; Modify opens it in the editor, and so does double-clicking it. They are not available for built-in filters.
- **Help**: opens this topic.

**Paste**, **Make Global** and **Copy As Layout** are shown but not available.

## Common tasks
1. Choose `View > Filter By > Filters...`.
2. Tick **Select** next to **Critical** and **Not Started**. **All Activities** clears by itself.
3. Choose **All selected filters** to see critical work that has not begun.
4. Click **OK**.

## Good to know
> Critical means the activity has no room to slip without moving the project finish.
> New, changed, copied and deleted filters are kept only when you click **OK** or **Apply**. Your own filters are kept in the workspace, for every project. The ticked filters belong to the current layout.
> If the table looks empty, check the filters first: **All Activities** brings everything back.

## See also
- [Filter editor](filter-editor)
- [Open Layout](open-layout)
- [Group and Sort](group-and-sort)

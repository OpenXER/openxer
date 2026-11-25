# Columns

Chooses which columns the activity table shows, and in what order. Choose `View > Columns > Customize...`, or right-click the table and choose **Columns...**. The `View > Columns` menu is available while the activity table is on screen.

## What is on it
- **Available Options**: every column you can add that is not shown yet, sorted into groups such as dates, durations and codes. Click a group to open or close it. Click the header and choose **Group and Sort By > List** for one alphabetical list, or **Category** for the groups.
- **Selected Options**: the columns shown now, top to bottom in the order they appear from left to right in the table.
- **>** adds the column picked in Available Options. **>>** adds every column the list shows (the open groups, or all of them in the flat list). Double-clicking a column adds it too.
- **<** removes the picked column from Selected Options; **<<** removes them all.
- **^** and **v**: move the picked selected column up or down.
- **OK**: applies the columns and closes. **Apply** applies them and keeps the window open. **Cancel** closes without changing the table.
- **Default**: puts back the standard column set (Activity ID, Activity Name, Original Duration, Remaining Duration, Schedule % Complete, Start, Finish, Total Float).
- **Edit Column...**: changes the picked selected column. It opens a small window with the **Default Title**, a **New Title** to show in the header instead, the **Width** (20 to 1500 pixels), and the **Alignment** (**Left**, **Center** or **Right**). Its **Default** button puts back the standard title, width and alignment. By default text and dates sit left, numbers right and Yes/No columns centered. The change shows in the table when you click OK or Apply in Columns.
- **Help**: opens this topic.

## Common tasks
1. Choose `View > Columns > Customize...`.
2. Open a group in **Available Options**, pick a column such as Total Float and click **>**.
3. Use **^** and **v** to put it where you want it.
4. Click **OK**.

## Good to know
> The table needs at least one column. Removing them all and clicking OK or Apply shows "Select at least one column."
> Columns belong to the current layout. Your changes stay on screen, but they are kept in the named layout only when you save it with `View > Layout > Save Layout`.
> The `View > Columns` menu also lists the column sets of the standard layout and your saved layouts, by layout name. Picking one puts those columns in the current layout.
> **Predecessor Details** and **Successor Details** (in the General group) list each relationship of the activity as its Activity ID, a colon and the relationship type, followed by the lag when it is not zero, as a plain number in your duration unit: for example `A1000: FS, A1010: SS 1`. They are read-only and go out with the Excel export.
> You can also drag a column edge in the table header to change its width.

## See also
- [Open Layout](open-layout)
- [Group and Sort](group-and-sort)
- [Table Font and Row](table-font-and-row)

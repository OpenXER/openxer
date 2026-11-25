# Group and Sort

Sorts the activities into bands (for example by WBS, the work breakdown of the project, or by a code) and sets the order inside each band. Choose `View > Group and Sort by > Customize...`, or right-click the table and choose **Group and Sort...**.

## What is on it
- **Display Options**: **Show Group Totals** adds totals to each band row. **Show Grand Totals** adds a total row for the whole table. **Show Summaries Only** shows the band rows and hides the activities.
- **Group By**: one row per band level, top level first. The blank row at the bottom adds a level. Click a cell of the selected row to change it.
  - **Group By**: the field to group on, such as WBS or an activity code.
  - **Indent**: indents the level under the one above. It is ticked for a new level.
  - **To Level**: for WBS, how many WBS levels deep to band (1 to 10), or **All**. Lower WBS levels show grayed under it.
  - **Group Interval**: for a date field, one band per **Day**, **Week**, **Month**, **Quarter** or **Year**; for a number field, a step size, such as 10. Leave it blank for one band per value.
  - **Page Break**: starts a new printed page at each band of this level.
  - **Font & Color**: click the sample of any level, the lower WBS levels included, to set that band's **Background**, **Text** color, **Size** (6 to 24) and **Bold**, then click **Close**.
- **Group By Options**: **Hide if empty** drops bands with no activities. **Sort bands alphabetically** orders bands by their text. **Show** ticks what the band row says: **Title**, **ID / Code** and **Name / Description**.
- **OK**, **Apply**, **Cancel**: apply and close, apply and stay, or leave unchanged.
- **Delete**: removes the selected level.
- **Sort...**: opens the **Sort** window to order activities inside each band. **Add** a row, then on the selected row pick its **Field Name** and set its **Sort Order** to **Ascending** or **Descending** from the drop-downs in the cells, and use **Move Up** and **Move Down** to say which field counts first. **OK** there hands the sort back to Group and Sort.
- **Default**: puts back the standard grouping (by WBS, sorted by Activity ID).
- **Help**: opens this topic.

## Common tasks
1. Choose `View > Group and Sort by > Customize...`.
2. In the first **Group By** row, pick WBS.
3. Set **To Level** to 2 to band only the top two WBS levels.
4. Click **Sort...**, click **Add**, set **Field Name** to Start and **Sort Order** to **Ascending**, and click **OK**.
5. Click **OK**.

## Good to know
> A number field with a bad **Group Interval** stops OK and names the field.
> Grouping belongs to the current layout. It is kept in the named layout when you save it with `View > Layout > Save Layout`.
> The `View > Group and Sort by` menu also lists the groupings of the standard layout and your saved layouts, by layout name.

## See also
- [Collapse To](collapse-to)
- [Columns](columns)
- [Filters](filters)
- [Open Layout](open-layout)

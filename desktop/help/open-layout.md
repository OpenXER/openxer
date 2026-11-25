# Open Layout

Switches the activity table and Gantt chart to a saved layout. A layout is a named set of view settings: columns, filters, grouping and sorting, table font and rows, bars, Bar Chart Options, the timescale, the bottom pane and the Activity Network and Trace Logic options. Choose `View > Layout > Open Layout...`. This topic also covers **Save Layout As**.

## What is on it
- **Activity Layouts**: the list of layouts, in one **Layout** column, grouped under bands. **Global** holds the layouts that come with OpenXER (the Standard Layout and the samples); **User** followed by your Windows user name holds the ones you saved.
- **Use filter selection saved with layout**: ticked when the window opens. When ticked, the layout's own filters come with it. Clear it to keep the filters you have now.
- **Open**: opens the picked layout and closes the window. Double-clicking a name does the same.
- **Cancel**: closes without changing the view.
- **Apply**: shows the picked layout but leaves the window open, so you can try several before you settle on one.
- **Delete**: removes the picked layout after you confirm it. It is not available for the Standard Layout.
- **Help**: opens this topic.
- **Import...**: adds a layout from an OpenXER layout file (.oxl). If its name is taken, the imported layout gets a number added.
- **Export...**: writes the picked layout to a .oxl file you can share.

## Save Layout As
`View > Layout > Save Layout As...` keeps the current view under a new name. Type the **Layout Name** (up to 100 characters) and click **Save**; **Cancel** leaves without saving, and **Help** at the left of the row opens this topic. A layout with the same name is replaced. `View > Layout > Save Layout` saves changes into the layout you are on.

## Common tasks
1. Set up the columns, filters and grouping the way you want them.
2. Choose `View > Layout > Save Layout As...`, type a name such as "Two-week look-ahead" and click **Save**.
3. Later, choose `View > Layout > Open Layout...`, pick the name and click **Open**.

## Good to know
> If the current layout has unsaved changes, OpenXER asks "Would you like to save the changes to this layout?" before opening another. That covers the table (columns, filters, grouping, table font and rows) and the Gantt chart's bars, bar chart options and timescale; zooming, scrolling and moving the splitters do not count. **Yes** saves first, **No** drops the changes, **Cancel** stays where you are.
> Layouts are kept in the workspace, so they are there for every project.
> Page setup for printing is saved per layout name too.

## See also
- [New Layout](new-layout)
- [Columns](columns)
- [Filters](filters)
- [Group and Sort](group-and-sort)
- [Page Setup](page-setup)

# Bars

Sets which bars the Gantt chart draws and how each one looks. Choose `View > Bars...`, or right-click the Gantt chart and choose **Bars...**. Both are available while the Gantt chart is on screen.

## What is on it
- **The bar list**: one row per bar type, drawn in list order. Click a row to edit it.
  - **Display**: tick to draw the bar, clear to hide it.
  - **Name**: the bar's name. Click it on the selected row to rename.
  - **Timescale**: the dates the bar is drawn from, such as **Current Bar**, **% Complete Bar**, **Plan Bar**, **Actual Bar**, **Remain Bar**, **Early Bar**, **Late Bar**, **Float Bar**, **Neg Float Bar** or a baseline bar. Click it on the selected row to choose.
  - **User Start** and **User Finish**: shown but empty.
  - **Filter**: which activities get this bar, such as **Critical**, **Milestone** or **Not Started**. Click it on the selected row (or double-click it) to choose; you can tick several and say whether an activity must match all or any.
  - **Preview**: a sample of the bar.
- **Bar Style** tab: the **Shape**, **Color** and **Pattern** of the start, middle and end of the bar, and its **Row** (1 to 3: which line within the activity row it sits on, so bars can stack).
- **Bar Settings** tab: **Show bar when collapsed** and **Show bar for grouping bands** decide whether the bar also shows on band rows. **Calendar nonwork time** narrows the bar over nonwork time such as weekends.
- **Bar Labels** tab: text to put beside the bar. Each label has a **Position** (**Left**, **Right**, **Top**, **Bottom** or **Center**) and a **Label** field, such as Activity Name, Finish or Total Float. **Add** adds a label (up to 8); **Delete** removes the selected label, or the last one when none is selected.
- **OK**, **Apply** and **Cancel**: apply and close, apply and stay, or leave unchanged.
- **Add** and **Delete**: add a new bar type after the selected one, or remove the selected one.
- **Shift up** and **Shift down**: move the selected bar in the list.
- **Options...**: opens [Bar Chart Options](bar-chart-options).
- **Default**: puts back the standard bars.
- **Help**: opens this topic.

## Common tasks
1. Choose `View > Bars...`.
2. Tick **Display** on the Primary Baseline Bar row to compare against the baseline.
3. On **Bar Style**, set its **Row** so it sits below the current bar.
4. Click **OK**.

## Good to know
> **Activity nonwork intervals** is shown but has nothing to act on yet.
> Bars belong to the current layout. They are kept in the named layout when you save it with `View > Layout > Save Layout`.

## See also
- [Bar Chart Options](bar-chart-options)
- [Chart Font and Colors](chart-font-and-colors)
- [Timescale](timescale)

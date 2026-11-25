# Profile Options

Sets what the usage profiles draw and how. Right-click the Activity Usage Profile or the Resource Usage Profile and choose **Profile Options...**, or choose `View > Bottom Layout Options...` while a profile is in the bottom pane. Each profile has its own series; the Graph tab is shared.

## Data tab
- **Units** or **Cost**: what the bars and curves measure.
- **Labor**, **Nonlabor**, **Material**, **Expenses** (Activity Usage Profile): which assignments count. Expenses are costs, so they count with **Cost** only. With more than one checked the legend says Total.
- **Show Bars/Curves**: one row per series. **By Date** draws the series as bars, one per period. **Cumulative** draws its running total as a curve on the right-hand scale. **Color** sets the series color with the color picker.
  - **Budgeted**: the budget over the planned dates.
  - **Actual**: the actual units up to the data date.
  - **Remaining Early** and **Remaining Late**: the remaining units over the early dates and over the late dates. Their colors are under **Remaining Bars**. The late curve is dashed, so the two curves show the band between early and late.
  - **Project Baseline** and **Primary Baseline** (Activity Usage Profile): the budget of the assigned baseline over the baseline's own planned dates, matched by Activity ID. The rows are available only when a stored baseline is assigned (`Project > Assign Baselines...`).
- **Show Limit**, **Show Overallocation**, **Availability** (Resource Usage Profile, units only): the limit line, units above it in the overallocation color, and the spare capacity under it.
- **Show Overtime** (Resource Usage Profile): the overtime part of the actual units in its own color.
- **Show Actual and Remaining**: actual and remaining early units stand in one bar; off, they are two bars side by side.

## Graph tab
- **Vertical Lines**: **Major** draws a line at each step of the timescale's upper row, **Minor** at each step of its lower row.
- **Horizontal Lines**: **Dotted**, **Solid** or **None** at the left scale's steps, in the **Line Color**.
- **Show Legend**: the box that names each color.
- **3D Bars**: bars drawn as blocks with a top and a side.
- **Show Values**: each bar's value written over it.
- **Background Color**: the chart's background.
- **Calculate Average**: each period's total on the left-hand scale (the bars and the limit) is divided by the number in **Divide each period by**. With **Use the work hours of each period** the divisor is the period's work hours on the Time Periods tab of `Admin > Admin Preferences...` (a day, week, month, quarter or year), which turns hours into people working full time. **Unit of Measure** labels the left-hand scale. The running totals keep their sums.

## Buttons
- **OK** applies and closes. **Cancel** or Escape closes without changes. **Apply** applies and keeps the dialog open. **Preferences...** opens User Preferences. **Help** opens this topic.

## Common tasks
1. Show the Resource Usage Profile with `View > Show on Bottom > Resource Usage Profile` and select a resource.
2. Right-click the profile and choose **Profile Options...**.
3. Check **By Date** for **Remaining Late** to see the late demand beside the early demand, and **Cumulative** for both to see the band between them.
4. Click **OK**.

## Good to know
> On a profile, **Cumulative** in the **Display** submenu of the right-click menu switches the curves of every series drawn as bars at once.
> A role selected in the list (**Select View > By Role**) shows staffed and unstaffed demand and has no limit line.
> These options are saved with the layout, and are not undo steps: they never change the project.

## See also
- [Usage Spreadsheets and Profiles](usage-views)
- [Level Resources](level-resources)

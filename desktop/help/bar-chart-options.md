# Bar Chart Options

Sets how the Gantt chart draws its lines, nonwork time and collapsed bands. Right-click the Gantt chart and choose **Bar Chart Options...**, or click **Options...** in the [Bars](bars) window.

## What is on it
- **General** tab:
  - **Show Relationships**: draws the logic lines between bars. Set the colors for **Driving** links (the ones that control the successor's date), **Non-driving** links and **Critical** links, and tick **Dotted non-driving** to draw non-driving links dotted.
  - **Show Legend**: draws a legend of the bars on the chart.
  - **Limit text label characters to**: cuts long bar labels to a number of characters (1 to 999).
  - **Default Size For Notebook Items**: a **Width** and **Height** in pixels (10 to 999). They are kept with the layout, but nothing on the chart uses them yet.
  - **Nonwork Time**: **Shade calendar nonwork time**, **Diagonal hatch** and the shade **Color**.
  - **Row height**: the height of each chart row, from 17 to 48.
- **Collapsed Bars** tab, under **Collapsed Bar Formatting**:
  - **Collapse to grouping bands**: a folded band shows its activities' bars on the band row. **Show relationships for collapsed bars** keeps the logic lines.
  - **Collapse to individual bars to minimize space**: packs the bars of a folded band tightly. **Allow overlapping of bar labels** is shown but not available, because collapsed bars draw without labels.
- **Data Date** tab: the **Style**, **Size** (1 to 5) and **Color** of the line drawn at the data date (the "as of" date of the schedule).
- **Sight Lines** tab: **Horizontal Lines** (**Show Major Lines**, which run along grouping bands, and **Show Minor Lines every** so many **Rows**) and **Vertical Lines** (**Show Major Lines every** and **Show Minor Lines every** so many time units, each with a **Start**). Each has a style and color.
- **Progress Line** tab: **Display progress line** with its **Size** (1 to 5) and **Color**, and the **Baseline to use for calculating Progress Line**. Under **Draw progress line**, choose **Based on difference between current and baseline activity's** start or finish date, or **By connecting progress points based on activity's** percent complete or remaining duration.
- **OK**: applies and closes. **Cancel**: closes without changes. **Help**: opens this topic.

## Common tasks
1. Right-click the Gantt chart and choose **Bar Chart Options...**.
2. On **Progress Line**, tick **Display progress line** and pick the baseline.
3. Click **OK**.

## Good to know
> **Shade calendar nonwork time** here and **Shade nonwork time** in Chart Font and Colors are the same setting. The sight line colors can be set in either place.
> **Row height** here and **Select height for all rows** in Table, Font and Row set the same height.
> `View > Progress Line` switches the progress line on and off without opening this window.
> These options belong to the current layout. They are kept in the named layout when you save it.

## See also
- [Bars](bars)
- [Chart Font and Colors](chart-font-and-colors)
- [Timescale](timescale)

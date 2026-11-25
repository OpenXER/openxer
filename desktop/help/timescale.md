# Timescale

Sets the date header above the Gantt chart: how many lines it has, where it starts, its font and how dates are counted. Choose `View > Timescale...`, or right-click the Gantt chart or its date header and choose **Timescale...**.

## What is on it
- **Timescale Format**: **Two Lines** or **Three Lines** of dates in the header.
- **Timescale Start**: where the chart begins. Click the **...** button, pick **Project Start (PS)**, **Data Date (DD)** or **Date** (then type the date), and set an offset in days. The data date is the "as of" date of the schedule.
- **Font & Color**: click the sample to set the header font size (6 to 16 pt), **Bold** and text color. **Default Font** puts the standard font back.
- **Show Primary Dates**: the header shows calendar dates.
  - **Type**: **Calendar**, **Fiscal** or **Week of Year**. With Fiscal, you also pick the month the fiscal year starts.
  - **Date Interval**: the pair of units the header shows, such as **Year/Month**, **Month/Week**, **Week/Day 1** or **Day/Hour**. **Automatic (follows zoom)** changes the units as you zoom in and out. **Day/Shift** is listed but not available. A new workspace starts on **Quarter/Month**.
  - The week start: **Sunday weeks** or **Monday weeks**.
- **Show Ordinal Dates**: the header counts periods (1, 2, 3...) instead of showing dates. Set the **Ordinal Start**, the date period 1 begins (with the same **...** button), and the **Ordinal Date Interval** (day, week, month, quarter or year).
- **OK**: applies and closes. **Apply**: applies and stays open. **Cancel**: closes without changes. **Help**: opens this topic.

**Financial Period Calendar** and **Shift Calendar** are shown but not available.

## Common tasks
1. Choose `View > Timescale...`.
2. Click the **...** button beside **Timescale Start**, pick **Data Date (DD)** and set the offset to -7 to start a week before the data date.
3. Choose **Show Primary Dates** and set **Date Interval** to **Month/Week**.
4. Click **OK**.

For a look-ahead that counts weeks from the data date, choose **Show Ordinal Dates**, set the **Ordinal Start** to **Data Date (DD)** and the interval to week.

## Good to know
> A typed date is used only once it reads as a date. Until then the field keeps the date it had.
> The printed timescale is set separately in [Page Setup](page-setup), on the Options tab.
> The timescale belongs to the current layout and is kept in it when you save it.

## See also
- [Bar Chart Options](bar-chart-options)
- [Bars](bars)
- [Page Setup](page-setup)

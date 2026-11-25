# Report Properties

Report Properties sets the details of one part of a report: the page, a data source, a row or a cell. Open it from the [Report Editor](report-editor): select a line in the outline and click **Properties...**. It opens on the tab that fits the selected line.

## What is on it
Tabs run down the left side. A tab that does not fit the selection is unavailable and says what to select first.
- **Report**: **Report title** and **Report name**. **Page Setup** has **Paper**, **Portrait** or **Landscape**, **Shrink to fit the page width**, **Scale %** (10 to 400) and **Margins (inches)**. **Header and Footer Text** has left, middle and right boxes for each; they can hold [date], [time], [page_number] and [total_pages].
- **Source**: **Loop through** picks the records the section lists. A data source that follows a **Join path** from the records above gets its records from that path, and **Loop through** is greyed out for it. **Organize hierarchically** lists them as a tree, with **Maximum level** and **Hide record if no children**. Also **Hide if empty**, **Page break** (None, After each record, After the set), **Filter (one line per condition)** and **Sort (field ASC or field DESC)**.
- **Row**: **Height**, **Hide if empty** and **Row filter (one line per condition)**.
- **Cell** (text cells): **Cell shows** picks Fixed text, Text with variables, Data field, Field title or Summary of a field, and the boxes below change to match. For a field, type its name in **Field** or choose it from the **Pick...** list of the nearest data source's fields. For a summary, **Total over** and **Function** say what to add up. Then **Left** and **Width**, **Text alignment**, **Text height (px)**, **Bold**, **Italic**, **Underline**, **Text color**, **Background**, **Borders**, **Hide if empty**, **Indent by hierarchy level** and **Repeat across the report's date range (one cell per period)**.
- **Line**: **Left**, **Width**, **Number of lines** and **Line alignment**.
- **Image**: **Image file** with **Select image...**, **Left**, **Width**, **Height** and **Image alignment**.
- **OK**: hands the changes back to the editor as one undo step. **Cancel** or `Escape`: drops them. **Help**: opens this topic.

## Common tasks
1. In the editor, select a text cell and click **Properties...**.
2. On the **Cell** tab set **Cell shows** to Data field and choose the field from the **Pick...** list.
3. Set **Width** and **Text alignment**, then click **OK**.

## Good to know
> Filter lines that cannot be read are counted on the tab. When the report runs they are ignored and listed as warnings.
> OK only updates the editor. The report is saved when you click Save or OK in the Report Editor.

## See also
- [Report Editor](report-editor)
- [Reports view](view-reports)

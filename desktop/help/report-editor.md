# Report Editor

The Report Editor changes the layout of a saved report, shown as an outline you change with the buttons and the Properties dialog. Open it from the Reports view: select a report, right-click it and choose **Modify...**, or click the Modify Report button on the Reports toolbar. To start an empty one, right-click and choose **Add Blank Report...**.

## What is on it
- **Report Layout** outline: every part of the report, indented by level: headers, footers, sections, data sources, rows and cells. Click a line to select it.
- **OK**: saves the report and closes the editor.
- **Cancel**: closes without saving the changes made since the last Save.
- **Save**: saves and keeps the editor open. It is enabled once something has changed.
- **Add Row**: adds a row to the selected header, footer or section, or to the one that holds the selected row or cell. With the report itself selected, the row goes in the page header.
- **Add Text Cell**, **Add Line Cell**, **Add Image Cell**: add a cell after the last cell of the selected row, or of the row that holds the selected cell.
- **Add Data Source**: adds a data source (the records a section lists, one line per record) to the selected section. A new data source lists activities until you change it in Properties.
- **Properties...**: opens [Report Properties](report-properties) for the selected line, where text, fields, sizes and filters are set.
- **Copy**, **Paste**: copy a row, cell or data source and paste it into the selected line, or the nearest line above it where that kind of part belongs.
- **Delete**: removes the selected row, cell or data source.
- **Move Up**, **Move Down**: move the selected row, cell or data source among its neighbours.
- **Undo**, **Redo**: step back or forward through your changes in this editor. `Ctrl+Z` and `Ctrl+Y` do the same here.
- **Preview...**: runs the report on the open project and shows its pages.
- **Help**: opens this topic.
- The line at the bottom shows Changed, Saved or Copied, or why the last command did not work.

## Common tasks
1. Select the section, header or footer and click **Add Row**.
2. With the new row selected, click **Add Text Cell**.
3. Click **Properties...** and set the cell's text or field.
4. Click **Preview...** to check the result, then **OK**.

## Good to know
> Changes are kept only when you click Save or OK. Escape or Cancel drops anything unsaved.
> Saving stores the report in your workspace. The **Report name** set in Properties becomes the report's name in the Reports view.
> **Add Blank Report...** saves the new, empty report straight away, so it stays in the list even if you cancel the editor.
> For a report made with the Report Wizard, **Modify...** opens the wizard instead of this editor.
> The editor's Undo and Redo cover only this report; they are separate from the project's undo.

## See also
- [Report Properties](report-properties)
- [Reports view](view-reports)
- [Print Preview](print-preview)

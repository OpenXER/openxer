# Import Reports

Import Reports brings report definitions into your workspace, so they show in the Reports view and can be run on your projects. Open it from the Reports view: right-click and choose **Import...**, or click the Import Report button on the Reports toolbar.

## What is on it
- **Import From**: where the reports come from.
- **A report definition file**: one report saved as a file, usually with the .rpt extension (.txt is also offered).
- **The reports in a database copy**: a copy of a scheduling database file (.db, .sqlite or .sqlite3) that holds many reports. You pick which ones in the next window.
- **File**: the path of the file. The **...** button picks it, offering the file types that fit your choice above.
- **OK**: imports the definition file, or opens the database and shows its reports in [Import Reports from a Database](import-reports-database). It stays disabled until a file is filled in.
- **Cancel**: closes without importing.
- **Help**: opens this topic.

## Common tasks
1. Choose **A report definition file**.
2. Click **...** and pick the file.
3. Click **OK**. The status bar names the imported report.

## Good to know
> A definition file is imported as a new report, named from the file. If that name is taken, a number is added. It goes into the report group selected in the Reports view, or the group of the selected report.
> If the file is not a report definition, or cannot be read, the reason shows in the dialog and nothing is imported.
> To bring in many reports at once, with their groups and batches, use the database choice.

## See also
- [Import Reports from a Database](import-reports-database)
- [Export Report](export-report)
- [Reports view](view-reports)

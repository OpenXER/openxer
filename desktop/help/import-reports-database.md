# Import Reports from a Database

This window lists the reports found in a database copy and lets you choose which ones to bring into your workspace. It opens after you pick **The reports in a database copy** in [Import Reports](import-reports) and click **OK**.

## What is on it
- **Report Name** list: the reports in the database, each with a check box. Click a row to check or clear it. **Type** shows the report type code stored in the database.
- **Show reports of every type**: off, the list shows only reports that users made. On, it shows every report in the database. When the database has no user-made reports, it starts switched on.
- **Import**: imports the checked reports and closes the window. It stays disabled until at least one report is checked.
- **Cancel**: closes without importing.
- **Select All**: checks every report in the list as it is shown.
- **Clear All**: clears every check.
- **Help**: opens this topic.

## Common tasks
1. Check the reports you want, or click **Select All**.
2. Click **Import**.
3. Read the status bar: it says how many reports, groups and batches came in.

## Good to know
> The report groups of the chosen reports come in with them. A group with the same name in the same place is reused, not copied.
> The saved batches that hold any of the chosen reports come in too, as Global batches that list only the reports you imported. A batch whose name is taken gets a number.
> A report whose text cannot be read is skipped; the status bar counts how many were skipped.
> If the workspace cannot store the reports (for example, the disk is full), nothing is imported: the reports, groups and batches already written by that import are taken back, a message says why, and the window stays open so you can try again.
> Imported reports are stored in your workspace. The database copy is only read, never changed.

## See also
- [Import Reports](import-reports)
- [Report Groups](report-groups)
- [Batch Reports](batch-reports)

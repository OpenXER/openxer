# Batch Reports

A batch is a set of reports written to files in one go, for example the weekly update package. This dialog picks the reports, their order, the folder and the file format, and can save the set by name for next time. Open it with `Tools > Reports > Batch Reports...` or the Batch Reports button on the Reports toolbar.

## What is on it
- **Report Group / Report** list: every saved report under its group, with a check box. Click a row to check or clear it; a group's box checks or clears every report under it. **Scope** shows whether a report is Global (for all projects) or Project (for one project).
- **Run Order**: the checked reports in the order they will run.
- **Output folder**: where the files go. The **...** button picks a folder. It starts at the folder of the last batch run, or your Documents folder.
- **Format**: HTML file, Excel file, PDF file or CSV file. PDF is the default.
- **Saved batch**: picks a saved batch, which checks its reports in its order and fills in its name and scope. It is unavailable until a batch has been saved.
- **Batch name** and **Report scope**: the name and scope (Global or Project) used by Save Batch. The scope starts as Global.
- **Close**: closes the dialog.
- **Run**: writes one file per checked report into the folder, in run order. Each file is named after its report. It stays disabled until a report is checked and a folder is filled in.
- **Select All**, **Clear All**: check or clear every report.
- **Move Up**, **Move Down**: move the report picked in Run Order.
- **Save Batch**: saves the checked reports, in run order, under the batch name. A batch with the same name is replaced.
- **Delete Batch**: deletes the batch chosen in Saved batch.
- **Help**: opens this topic.

## Common tasks
1. Check the reports you want.
2. Pick a row in **Run Order** and use **Move Up** or **Move Down** to arrange them.
3. Set the **Output folder** and **Format**, then click **Run**. A line under the fields says how many files were written.
4. To keep the set, type a **Batch name**, choose the **Report scope** and click **Save Batch**.

## Good to know
> Only definition reports (Subject Area Definition or OpenXER) can be saved in a batch. Reports made with the Report Wizard can be run here but not saved in one.
> A Global batch takes only global reports, and a Project batch only reports of the open project.
> Definition reports cannot be written as Excel files; use HTML, CSV or PDF.
> A project report of another project stops the run before anything is written, and a message names the project to open.
> Files with the same name in the folder are replaced.

## See also
- [Run Batch](run-batch)
- [Report Groups](report-groups)
- [Reports view](view-reports)

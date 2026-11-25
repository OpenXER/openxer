# Run Batch

Run Batch writes every report of a saved batch to files, in the order the batch lists them. Open it from the Reports view: right-click a report and choose `Run > Batch...`, or click the Run Batch button on the Reports toolbar.

## What is on it
- Batch list: the saved batches this project can run, with **Batch Name**, **Report Scope** and the number of **Reports** in each. Global batches run on any project; a Project batch shows only on its own project.
- **Output Folder**: where the files go. The **...** button picks a folder. It starts at the folder of the last batch run, or your Documents folder.
- **Format**: HTML file, ASCII text file or PDF file. HTML file is the default. The text file is comma separated, with each field in double quotes, and gets the .csv extension.
- **User's Notes**: a line of text passed to the reports, for those that print the user's notes.
- **Run**: writes one file per report into the folder, named after the report, and closes the dialog. It stays disabled until a batch is picked and a folder is filled in.
- **Cancel**: closes without running anything.
- **Help**: opens this topic.

## Common tasks
1. Click the batch you want.
2. Check the **Output Folder** and pick a **Format**.
3. Type any **User's Notes**, then click **Run**. The status bar says how many reports were written and where.

## Good to know
> Batches are made and saved in [Batch Reports](batch-reports). This dialog only runs them.
> If a report in the batch cannot run on this project, nothing is written and the reason shows under the fields.
> Files with the same name in the folder are replaced. Two reports with the same name in one batch get a number, such as Weekly (2).

## See also
- [Batch Reports](batch-reports)
- [Run Report to File](run-report)
- [Reports view](view-reports)

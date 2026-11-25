# Run Report to File

This dialog runs one report on the open project and saves the result as a file. Open it from the Reports view: select a report, right-click it and choose `Run > Report to File...`.

## What is on it
- **Report**: the name of the report you are running.
- **Send Report To**: the kind of file to write. HTML File is picked to start.
- **HTML File**: a web page you can open in any browser.
- **ASCII Text File**: plain delimited text with the .txt extension, handy for a spreadsheet.
- **PDF File**: pages ready to print or send.
- **Output File**: the full path of the file. It starts as the report's name in your Documents folder. The **...** button picks it with a save box. Changing the file kind keeps the folder and name and changes the extension.
- **Field Delimiter**: Tab (the default), Comma, Semicolon, Space or Pipe between fields. Text files only.
- **Text Qualifier**: Double quote (the default), Single quote or None around each field. Text files only.
- **View file when done**: opens the file in its usual program after it is written.
- **User's Notes**: a line of text passed to the report, for reports that print the user's notes.
- **Run**: writes the file and closes the dialog. If the file cannot be written, the reason shows in the dialog and it stays open.
- **Cancel**: closes without writing anything.
- **Help**: opens this topic.

## Common tasks
1. Pick the file kind under **Send Report To**.
2. Check the **Output File** path, or click **...** to choose another.
3. For a text file, pick the **Field Delimiter** and **Text Qualifier**.
4. Tick **View file when done** if you want to see it at once, then click **Run**.

## Good to know
> A project report runs only on the project it belongs to. On another project, `Run > Report to File...` is unavailable for it.
> An existing file with the same name is replaced without asking.
> To see a report on screen instead, choose `Run > Report` in the Reports view; it opens in a tab.

## See also
- [Reports view](view-reports)
- [Report tab](view-report-tab)
- [Run Batch](run-batch)

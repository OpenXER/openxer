# Export

Writes projects from the workspace to an XER file, a project XML file or a spreadsheet. Choose `File > Export...`. Move through the pages with **Next** and **Prev**; **Cancel** leaves without writing a file.

## What is on it
- **Export Format**: **Schedule - (XER)** for exchange with other scheduling tools; **Project - (XML)** for tools that take the XML project exchange format; **Spreadsheet - (XLSX), OpenXER layout** for a readable workbook with real dates, outline and a frozen header; or **Spreadsheet - (XLSX), XER table layout** with the raw TASK and TASKPRED sheets.
- **Projects To Export**: every workspace project. Tick **Export** for each one to write; the open projects start ticked. **Baselines to Export** adds all of the project's stored baselines (XER and XML only, and only for a ticked project that has some). **Source** shows where the project came from.
- **Export Options** for XER: **Version** sets the version label (and today's date) in the file header. Only the label changes; the tables written are the same for every version.
- **Export Options** for XML: nothing to choose. The page lists what the file carries.
- **Export Options** for spreadsheets: **Durations, float and lag in** **Days** or **Hours** (float: how many days an activity can slip before it delays the finish). The OpenXER layout adds **Date format**, **Group by WBS** and a **Template** list with **New...**, **Edit...** and **Delete**. A saved template sets the columns, rows, unit and date format, so those choices are hidden; **Current table layout** uses the columns and filtered rows of the activity table for the open project.
- **File Name**: the path of the file to write. It starts in your Documents folder, named after the first project. **Browse...** picks another place.
- **Finish**: writes the file.

## Common tasks
1. Choose `File > Export...`, keep **Schedule - (XER)** and click **Next**.
2. Tick **Export** for each project to include, and **Baselines to Export** if needed.
3. Pick the **Version** and click **Next**.
4. Check the **File Name** and click **Finish**.

## Good to know
> Opening the wizard saves unsaved changes first, so the file holds what is on screen.
> A spreadsheet holds one project only. Tick just one to export as XLSX.
> Exporting never changes the workspace.
> An XML file carries projects with their schedule options, WBS, activities with their dates, durations, constraints and progress, relationships with lag, calendars with work weeks and exceptions, resources with their codes, assignments, activity codes, user defined fields, expenses, notebook topics and notes, the OBS and currencies, plus cost accounts, expense categories, units of measure, roles, rates and baselines when the project has them. Steps, documents, risks, issues and the other XER tables are left out; use XER to keep everything.
> What has been checked: an XML file exported by the scheduling product that defines this format opens in OpenXER. The files OpenXER writes follow the structure of that export (the root element, the order of every element's fields, fractions for percents, 1 and 0 for yes and no, hours for durations and lags), but they have not yet been opened in that product.

## See also
- [Import](import-wizard)
- [Columns](columns)
- [Maintain Baselines](maintain-baselines)

# Import

Brings projects from XER or project XML files into the workspace, or updates the open project from a spreadsheet. Choose `File > Import...`. Move through the pages with **Next** and **Prev**; **Cancel** leaves without importing.

## What is on it
- **Import Format**: pick **Schedule - (XER)**, **Project - (XML)** or **Spreadsheet - (XLSX)**. After an import, this page also says when the last one ran and shows an **Undo Last Import** button.
- **Select XER File**: type the **File Name**, or click the **...** button inside the box to pick the files. Several files can be listed, separated by ";". Each file is read as XER or XML by what it holds, so an XER file and an XML file can be imported together.
- **Import Project Options**: a list with one row for each project found in the files: **Import Action**, **Project ID**, number of **Activities**, **File** and **Import To**. Click a row, then pick its **Import Action** and **Import To** from the drop-downs in its cells (the action starts on Create New Project). **Import To** starts on the matching workspace project (same Project ID and internal project key), when there is one.
  - **Create New Project**: adds it as a new workspace project.
  - **Update Existing Project**: merges it into the project chosen under **Import To**.
  - **Replace Existing Project**: swaps the chosen project for the one in the file.
  - **Add Into Existing Project**: adds its activities into the chosen project.
  - **Ignore this Project**: skips it.
- **EPS folder for new projects**: the folder of the Projects view that projects set to **Create New Project** are filed in. It starts on the folder selected in the Projects view (or the folder of the selected project), else the first top folder. Projects that update, replace or add into an existing project keep the folder they are in.
- **Open the imported project when the import finishes**: on by default.
- **Update Project Options**: shown only for Update or Add Into. A list with one row for each **Data Type** (activities, relationships, WBS, resource assignments, calendars, resources and more): click a row and pick its **Import Action** from the drop-down in the cell, **Keep Existing**, **Update Existing** or **Do Not Import**. "(global)" marks data shared by all projects; those types also offer **Insert New**. The **Delete** check box, offered for activities, relationships and resource assignments, removes project data that is not in the file; click it to tick or clear it.
- **Ready to Begin Importing Data**: a list of what each project will get, a line each, with warnings. **Finish** runs the import. It is not available while any project shows "Cannot import".

## Common tasks
1. Choose `File > Import...`, pick **Schedule - (XER)** and click **Next**.
2. Click the **...** button in the **File Name** box, pick the file and click **Next**.
3. Set the **Import Action** and, where asked, the **Import To** project.
4. Review the summary and click **Finish**.

## Good to know
> The workspace is backed up before anything is written. A message gives the backup location.
> To take an XER import back, use **Undo Last Import** on the first page. The new projects leave their folder with them.
> Two projects in the files cannot update or replace the same workspace project.
> Baselines found in the file are stored with their projects.
> A project XML file goes through the same pages and actions as an XER file. It carries projects with their schedule options, WBS, activities, relationships, calendars, resources with their codes, assignments, activity codes, user defined fields, expenses, notebook topics and notes, the OBS and currencies (and roles, rates, cost accounts and baselines when the file has them); anything else an XER would hold is not in it. Values OpenXER works out for itself, such as costs at completion, are not read.
> XML files exported by the scheduling product that defines this format have been checked to open here. To see what a file holds that OpenXER skips, run `openxer-cli inspect` on it: it lists the skipped element types and fields.
> The spreadsheet choice opens a separate three-page wizard: pick the file, check how its columns match project fields, then review every changed row before **Finish**. That import is one undo step.
> In a spreadsheet, a blank cell changes nothing: the field keeps the value it has. To clear an activity code, a user defined field, a constraint or a date such as an actual or suspend date, type `<none>` in the cell.
> A spreadsheet cell is checked by the same rules as typing it into the activity table. A value the table would refuse makes its row an error that names the column and cell, and that row is not imported.

## See also
- [Export](export-wizard)
- [Open Project](open-project)
- [Maintain Baselines](maintain-baselines)

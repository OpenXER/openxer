# User Defined Fields

User defined fields are extra fields you add yourself, such as a permit number or a punch-list date, to activities, resources, WBS elements and other records. Open this dialog with `Enterprise > User Defined Fields...` or from the Dictionaries toolbar.

## What is on it
- **Subject area** drop-down (under the heading): picks the kind of record whose fields are listed. The choices are Activities, Activity Expenses, Activity Steps, Issues, Project Risks, Projects, Resource Assignments, Resources, WBS, and Work Products and Documents.
- Fields list:
- **Title**: the field's name, up to 60 characters and unique within the subject area. The list is sorted by title. Click it on the selected row to rename it in place; Enter keeps the change, Escape drops it.
- **Data Type**: what the field holds: Text, Start Date, Finish Date, Cost, Integer, Number, Indicator or Code. On the selected row it is a drop-down while the field holds no values yet.
- **Close**: closes the dialog.
- **Add**: adds a Text field titled "(New)" to the chosen subject area and starts editing its title.
- **Delete**: removes the selected field. If it holds values, you are asked first.
- **Help**: opens this topic.

## Common tasks
1. Pick the subject area, for example Activities.
2. Click **Add** and type the title, then press Enter.
3. Pick the **Data Type** on the selected row.
4. Show the field as a column in the view, or fill it in on the User Defined Fields tab of the activity details.

## Fields as columns in the other windows
The WBS, Projects, Resources and Resource Assignments windows, and the Expenses, Issues, Risks and WPs & Docs views, show the fields of their own subject area as columns:
1. Open the window's column chooser: **Columns...** (from the layout bar or the right-click menu), or **Spreadsheet Options...** in Resource Assignments.
2. The fields are listed under **User Defined** (in the Category view of Available Options). Move one to Selected Options and click **OK**.
3. Click a row, then click its cell in the new column (or press F2) to change the value. Text, dates and numbers are typed; Enter keeps the value and Escape drops it. An indicator or a code is picked from the list under the cell.

Each change is one step for Undo. Dates are typed in your date format, a cost in your currency format, and an Integer field takes whole numbers only. In the Projects window only the open project's value can be changed. Where a window sorts on a column (Projects, Resources), clicking the field's column title sorts on it; Fill Down and Replace work on these columns where the window offers them.

## Good to know
> Choose the data type before you enter any values. Once a field holds values, its type is shown but cannot be changed.
> Deleting a field that holds values deletes those values too.
> Renaming an Activities field keeps it everywhere it is used: the columns, groups and sorts of the current layout and of every saved layout, the bottom table's columns, and your filters. The other windows keep their columns of a renamed field too: the column shows the new title.

## See also
- [Columns](columns)
- [Activities view](view-activities)

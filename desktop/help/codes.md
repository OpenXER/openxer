# Activity, Project, Resource and Assignment Codes

Codes are your own labels for grouping and filtering, such as area, phase or craft. Each code has a list of values, and each record can take one value per code. The four dialogs work alike; open them from the Enterprise menu (for example `Enterprise > Activity Codes...`) or the Dictionaries toolbar.

## What is on it
- **Global**, **EPS**, **Project** (Activity Codes only): which codes to list. Global codes serve all projects, EPS codes the projects in a folder, Project codes the open project only. The dialog opens on the first of Project, Global and EPS that has codes.
- **Select** drop-down (for example **Select Activity Code**): picks the code whose values are listed.
- **Modify...**: opens the code definitions window described below.
- Values list: **Code Value** and **Code Description** (**Description** in Activity Codes), shown as a tree. Activity Codes adds a **Color** column; click the selected row's swatch to pick a color. Click a cell of the selected row to edit it in place, or press `F2` to edit its Code Value. A Code Value cannot be blank, must be unique among values at the same level, and cannot be longer than the code's Max Length.
- **Close**, **Add** (adds a value named New after the selected one and starts editing it), **Delete**, **Cut**, **Copy**, **Paste**, the **arrow pad** (up and down reorder; right indents under the value above; left outdents) and **Help**.

## Code definitions (Modify...)
This child window lists the codes themselves.
- **Activity Code Name** (or Project, Resource or Assignment Code Name) column: the code's name, up to 60 characters and unique. Click it on the selected row to edit it in place.
- **Max Length**: the longest value allowed, from 1 to 20. It cannot be set below the longest value the code already has.
- **Close**, **Add** (adds a code named New Code, with a Max Length of 20, in the chosen scope), **Delete**, **Shift up** and **Shift down** (change the order of the codes) and **Help**.
- **Make Global** and **Make EPS** (Activity Codes only): widen a code's scope, from Project to EPS to Global. A code cannot be narrowed.

## Common tasks
1. Click **Modify...** and click **Add**. Click the new code's name and type a name, then click its **Max Length** to change it. Close the window.
2. Pick the code in the **Select** drop-down.
3. Click **Add**, type the **Code Value** and press Enter, then click the description cell and type the description.

## Good to know
> Deleting a value also deletes its child values and removes them from every record that has them, after you confirm.
> Deleting a code deletes all its values and assignments; if it has any, you are asked first.
> Renaming an activity code keeps it in the current layout's columns and groups.

## See also
- [Activities view](view-activities)
- [Group and Sort](group-and-sort)
- [Filters](filters)

# Role Codes

Role codes are your own labels for grouping roles, such as craft or union local. Each role code has a list of values, and each role can take one value per code. Open this dialog with `Enterprise > Role Codes...`.

## What is on it
- **Select Role Code**: the drop-down that picks which role code's values the list shows.
- **Modify...**: opens Role Code Definitions, where the role codes themselves are kept.
- **Code Value**, **Code Description** list: the values of the chosen code as a tree. Click a cell of the selected row to edit it; Enter keeps the change, Escape drops it. A Code Value must be unique within the role code and no longer than its Max Length.
- **Close**, **Add** (adds a value after the selected one), **Delete**, **Cut**, **Copy**, **Paste** (pastes after the selected value), the **arrow pad** (reorder, indent, outdent) and **Help**.
- Right-click a row for Add, Delete, Cut, Copy and Paste.

## Role Code Definitions
This child window lists the role codes.
- **Role Code**: the code's name, edited in place.
- **Max Length**: the longest value the code allows, a whole number from 1 to 20. It cannot be set shorter than a value the code already has.
- **Close**, **Add** (adds a new code with a Max Length of 10) and **Delete** (removes the code and all its values, after asking).

## Common tasks
1. Click **Modify...**, click **Add**, and name the new code. Close the window.
2. Pick the code in **Select Role Code**.
3. Click **Add** and type each **Code Value** and **Code Description**.
4. Assign values to roles on the Codes tab of [Roles](roles).

## Good to know
> **Add** stays disabled until at least one role code exists.
> Deleting a value also deletes the values under it and removes them from every role that has them, after you confirm.
> Escape closes Role Code Definitions first, then the Role Codes dialog.

## See also
- [Roles](roles)

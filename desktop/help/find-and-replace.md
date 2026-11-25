# Find and Replace

Finds text in the activity table, the WBS table or the Resources table, and can replace it. Choose `Edit > Find...` (`Ctrl+F`) for the **Find** window, or `Edit > Replace...` (`Ctrl+H` or `Ctrl+R`) for the **Replace** window. `Edit > Find Next` (`F3`) repeats the last search.

## What is on it
- **Find what**: the text to look for. Press `Enter` here to find the next match.
- **Replace with**: the new text (Replace window only).
- **Look in**: **All displayed columns**, or one column of the current layout.
- **Match case**: when ticked, capital and small letters must match exactly.
- **Find Next**: selects the next activity with a matching cell and moves to that cell. The status bar names the column it was found in.
- **Replace**: replaces the text in the current cell if it matches, then finds the next match (Replace window only).
- **Replace All**: replaces every match in the activities shown (Replace window only). The status bar says how many were replaced.
- **Close**: closes the window. The search is kept for `F3`.

## Common tasks
1. Press `Ctrl+F` and type part of an activity name or ID.
2. Click **Find Next** to jump to it. Press `F3` for the next one, even after the window is closed.

To rename a word in many activity names:
1. Press `Ctrl+H`.
2. Type the old word in **Find what** and the new one in **Replace with**.
3. Set **Look in** to the activity name column.
4. Click **Replace All**.

## In the WBS and Resources tables
The same windows work in the WBS window and the Resources window. **Look in** lists that table's columns, and **Find Next** selects the next row with a match. **Replace** works on the cell last clicked in the current row; **Replace All** on every row. Only names and IDs can be replaced there: WBS names, and resource IDs and names. Each **Replace** or **Replace All** is one undo step.

The Resources table also has Fill Down (`Ctrl+E`, `Edit > Fill Down` or its right-click menu): select two or more resources, and the cell you clicked first is copied to the others in that column. The Projects table has it on the **Project Name** column only. The WBS table has no Fill Down.

## Good to know
> Only activities shown in the table are searched. Activities hidden by a filter or a collapsed group are skipped. With several projects open, only the active project's activities are searched.
> The search starts after the selected activity and wraps around to the top.
> Replace works only in columns of plain text you can type in. Dates, numbers and lists are found but not replaced. If a new value is refused, the status bar counts it as rejected.
> The search looks at the text as it is shown in the cell, so dates match their displayed format.

## See also
- [Spell Check](spell-check)
- [Columns](columns)
- [Filters](filters)
- [Keyboard shortcuts](keyboard-shortcuts)

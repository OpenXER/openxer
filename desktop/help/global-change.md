# Global Change

Keeps a list of saved rule sets that change many activities at once, and runs them. Each rule set says "if an activity matches these conditions, then set these values". Choose `Tools > Global Change...`, or click the Global Change toolbar button.

## What is on it
- **Name**: the list of your saved global changes. Click one to pick it; double-click to modify it.
- **Close**: closes the window.
- **Apply Change**: runs the picked global change and shows the **Global Change Report** with every value it would change. Nothing is changed yet. Started from an Activities view, it looks only at the activities that view's filter shows, even when the rule has no If rows; activities the filter hides are left alone. With All Activities showing, or from any other view, it looks at every activity of the active project.
- **New...**: opens [Modify Global Change](global-change-edit) on a new rule set with one empty If row and one empty Then row.
- **Delete**: removes the picked global change from the list at once, without asking.
- **Modify...**: opens the picked global change for editing.
- **Copy** and **Paste**: Paste adds a copy of the global change you copied, named after it with the first free number on the end (a copy of "Shift dates" is "Shift dates1", the next "Shift dates2"), so you can make a variation.
- **Import** and **Export**: read global changes from a .json file (any that are not valid are skipped), or write the picked one to a .json file to share or keep.
- **Help**: opens this topic.

## Global Change Report
The top of the report names the global change, the projects it looked at and today's date. Below that is one row per change with **Selected**, **Project ID**, **WBS Code**, **Activity ID**, **Field**, **Old Value**, **New Value** and an **Exception** when a value cannot be set. Clear the **Selected** box of a row to skip that change. A row with an exception cannot be selected.
- **Select All** and **Clear All**: tick or clear every row at once.
- **Save Report**: when ticked (the default), the report is written to the file named beside it as soon as you click **Commit Changes**. The file starts as GlobalChangeReport.txt in your Documents folder; type another path or click **...** to choose one. **As HTML** writes a web page instead of tab-separated text and switches the file's extension. With **Overwrite Existing** cleared, a file that is already there is kept and the report gets the next free numbered name (GlobalChangeReport1.txt). The choices are remembered.
- **Save to File...**: saves the report now, to a file you choose, without making any change.
- **Cancel Changes**: closes the report and changes nothing.
- **Commit Changes**: makes the selected changes. All of them together are one undo step.
- **Help**: opens this topic.

## Common tasks
1. To change only some activities, filter the Activities view to them first (`View > Filter By > Filters...`).
2. Choose `Tools > Global Change...`.
3. Click **New...**, build the rule and click **OK**.
4. Pick it in the list and click **Apply Change**.
5. Check the report, clear any row you do not want, and click **Commit Changes**.
6. Press `F9` to schedule with the new values.

## Good to know
> Each change is tested with the same rules as typing in a table cell. A value the cell would refuse shows a message in the report and is not made.
> The status bar says how many values changed and how many could not be changed.
> Saved global changes are kept in the workspace, so every project can use them.

## See also
- [Modify Global Change](global-change-edit)
- [Find and Replace](find-and-replace)
- [Filters](filters)

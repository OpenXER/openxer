# Categories and Lists

This dialog keeps the short pick lists the rest of the program offers in drop-downs, such as expense categories and units of measure. Open it with `Admin > Categories and Lists...`. It also opens from a resource's unit of measure picker, with a **Select** button added.

## What is on it
- Tabs down the left side, one per list:
- **Baseline Types**: labels for kinds of baselines.
- **Expense Categories**: kinds of expenses, such as materials or rentals. The dialog opens on this tab.
- **Document Categories** and **Document Status**: labels for work products and documents.
- **Risk Categories**: kinds of risks. This list is a tree.
- **Notebook Topics**: headings for notes. The **EPS**, **Proj**, **WBS** and **Act** check boxes mark where each topic applies: folders, projects, WBS elements or activities. New topics have all four ticked. **Act** decides which topics an activity's Notebook tab offers; the other three are kept and written to the XER file.
- **Units of Measure**: **Unit Abbreviation** and **Unit Name**, used for material resources. When the dialog opens from a resource's unit picker it starts on this tab.
- The list of the chosen tab. Click a cell of the selected row to edit it in place; Enter keeps the change, Escape drops it. A name cannot be blank and must be unique among entries at the same level.
- **Add**: adds an entry after the selected one and starts editing it.
- **Delete**: removes the selected entry.
- **Shift up**, **Shift down**: move the entry among the entries at its level.
- **Shift Left**, **Shift Right** (Risk Categories only): move a category out one level, or under the one above.
- **Select** (only when opened from a picker), **Help** and **Close** along the bottom.

## Common tasks
1. Click the tab of the list you want.
2. Click **Add** and type the new entry, then press Enter.
3. Use **Shift up** and **Shift down** to put it in order.

## Good to know
> An entry that nothing uses is deleted at once. If records use it, you are asked first.
> Deleting a notebook topic deletes the notes filed under it.
> Deleting a baseline type leaves the baselines that had it without a type; other lists leave the records that used the entry blank.

## See also
- [Expenses view](view-expenses)
- [Documents view](view-documents)
- [Risks view](view-risks)
- [Maintain Baselines](maintain-baselines)

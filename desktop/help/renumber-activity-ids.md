# Renumber Activity IDs

Gives new Activity IDs to the selected activities in one go. Select the activities in the table, then choose `Edit > Renumber Activity IDs`, or the same item on the table's right-click menu. This topic also covers the **Duplicates** window.

## What is on it
Pick one of three ways to number:
- **Increment Activity ID based on selected activities**: the first selected activity keeps its ID. Each next one gets the number of the one before plus the **Increment Value** (10 to start). For example A1000, A1010, A1020.
- **Auto-number**: builds each ID from a **Prefix** and a starting number (**Suffix**), then adds the **Increment Value** for each next activity. The suffix must be digits; leading zeros are kept. Until you set them, they come from the project's automatic numbering settings. For example prefix "P-" and suffix 100 give P-100, P-110 and so on with an increment of 10.
- **Replace beginning characters**: swaps the first **Number of characters** of each ID for the text in **Replace with**. For example 2 characters replaced with "EL" turns PI-200 into EL-200.

Also on the window:
- **Renumber selected activities in baselines**: gives the same new IDs to these activities in the project's baselines (saved copies of the schedule), so comparisons still match. Shown grayed when the project has no baselines.
- **The status line**: how many activities change, what the first becomes, and how many new IDs are taken.
- **OK**: renumbers. It is available only when the rule can run.
- **Cancel**: closes without changes. **Help**: opens this topic.

Activities are numbered in the order they show in the table.

## Duplicates
If some new IDs are already taken, OK opens **Duplicates** instead of renumbering. It lists each **Duplicate Activity ID** with its **Activity Name** and a **New Activity ID** box. Type a free ID for each one. **OK** becomes available once there are no clashes left, and then renumbers everything. **Cancel** stops without changes.

## When pasting activities
Pasting copied activities (`Edit > Paste` after `Edit > Copy`) opens this window after **Copy Activity Options**, to give the copies their IDs:
- **Increment Activity ID based on selected activities** (the first choice): counts on from the first copied ID by the **Increment Value**. When that ID is already in use, the copy takes the highest number in use with the same prefix plus the increment, so the paste never stops for a clash. For example, copying A1000 by 10 when A1010 exists and A1340 is the highest gives A1350.
- **Auto-number**: counts from the **Prefix** and **Suffix** by the **Increment Value**, passing over IDs already in use.
- **Replace beginning characters**: changes the start of each copied ID. If a new ID is already used, **Duplicates** opens so you can type a free one; the copies are pasted when you click **OK** there. **Cancel** there asks whether to go back and give the duplicates new IDs: **Yes** (the default) returns to the list, **No** pastes nothing.
- **Skip this dialog next time**: later pastes skip the window. Each copy then keeps its own ID when it is free, else gets the project's next automatic ID.

**Cancel** stops the paste: nothing is added.

Pasting a copied WBS element with its activities in the WBS view opens this window too, after **Copy WBS Options** and **Copy Activity Options**, for the copies of those activities.

## Common tasks
1. Sort the table in the order you want, select the activities, and choose `Edit > Renumber Activity IDs`.
2. Choose **Increment Activity ID based on selected activities** and set the **Increment Value** to 10.
3. Check the status line and click **OK**.

## Good to know
> Every Activity ID in a project must be unique.
> The renumber is one undo step, baselines included. Your choices are kept for next time when you click **OK**.
> If a new ID is already used in a baseline, that baseline is left as it was and the status bar names it.

## See also
- [Copy Activity Options](copy-options)
- [Global Change](global-change)
- [Maintain Baselines](maintain-baselines)

# Cost Accounts

Cost accounts are the codes your company uses to track where money and hours go, kept as a tree. Expenses, resource assignments and the project's default all name a cost account. Open this dialog with `Enterprise > Cost Accounts...` or from the Dictionaries toolbar.

## What is on it
- Accounts list: **Cost Account ID** and **Cost Account Name**, shown as a tree. Click a cell of the selected row to edit it in place; Enter keeps the change, Escape drops it. The ID must be unique among accounts at the same level (up to 40 characters), and the name cannot be blank.
- Detail panel under the list, for the selected account:
- **Cost Account ID** and **Cost Account Name**: the same values as text boxes, saved when you leave the box.
- **Cost Account Description**: free notes, saved when you leave the box.
- **Modify**: puts the cursor in the description. **Copy**: copies the description to the clipboard. **Print** is shown but not available.
- **Close**: closes the dialog.
- **Add**: adds "New Account" after the selected one, at the same level, and starts editing its ID.
- **Delete or Merge...**: deletes the selected account, or merges it into another one (see below).
- **Cut**, **Copy**, **Paste**: move or duplicate an account with its children. Paste puts it after the selected account.
- **Arrow pad**: up and down reorder; right makes the account the last child of the account above it at the same level; left moves it out one level.
- **Help**: opens this topic.

## Common tasks
1. Select an account at the level you want and click **Add**.
2. Type the ID, press Enter, then fill in the name and description.
3. Use the arrow pad to put it under the right parent.

## Merging an account
If the account (or one under it) is in use, **Delete or Merge...** asks whether to pick a replacement.
1. Click **Yes** to open **Select Replacement Cost Account**.
2. Pick the account that should take over, then click **Select** (or double-click it).
3. Click **No** instead to delete the account and leave those records without a cost account, or **Cancel** to keep it.

## Good to know
> An account that is not in use and has no children is deleted at once, without a question. Use `Edit > Undo` if that was a mistake.
> An account with child accounts but no uses is deleted with its children after you confirm.

## See also
- [Expenses view](view-expenses)
- [Assignments view](view-assignments)

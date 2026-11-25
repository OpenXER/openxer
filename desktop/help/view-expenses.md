# Expenses

The Expenses view lists every expense item in the open project. An expense item is a cost that is not a resource, such as a rental, a permit or a purchased part, and it always belongs to one activity. Open it with `Project > Expenses` or the **Expenses** button on the Project bar.

## What it shows
- **Layout: Expenses**: the strip above the table. Click it to choose **Group by WBS**, **Group by Activity** or **No Grouping**, to **Expand All** or **Collapse All**, and to show or hide the details pane with **Details**.
- **Expense table**: one row per item with **Activity ID**, **Activity Name**, **Expense Item**, **Expense Category**, **Budgeted Cost**, **Actual Cost**, **Remaining Cost**, **At Completion Cost**, **Accrual Type** and **Vendor**.
- **Command bar**: **Add** and **Delete** on the right edge.
- **Expense Details**: tabs for the selected item.
- **General**: the item name, category, accrual type (when the cost is booked over the activity), unit of measure, vendor, PO number and **Auto Compute Actuals**. The cost account is shown but cannot be changed here.
- **Activity**: the activity the item belongs to, with its WBS, status and dates. **Activity...** picks a different activity.
- **Cost**: budgeted units, price per unit, budgeted, actual and remaining cost. At completion cost (actual plus remaining) and percent complete are worked out for you.
- **Description** and **User Defined Fields**.

## Common tasks
1. Select an expense already on the activity, or select the activity in the Activities view.
2. Click **Add** (or press `Insert`). A new item is added to that activity; with neither selected, it goes on the first activity.
3. Name it and set its category on the **General** tab, then enter the costs on the **Cost** tab.
4. To remove an item, select it and click **Delete** (or press `Delete`).

Right-click a row for **Add**, **Delete**, **Expense Details** and **Hide Detail Window**.

## Good to know
> **Add** is not available until the project has at least one activity.
> These are the same records as the **Expenses** tab of Activity Details, so a change in either place shows in both.
> **Customize Columns...** on the right-click menu picks and orders the columns; the choice is kept with the view layout.

## See also
- [Activities](view-activities)
- [Cost Accounts](cost-accounts)
- [Categories and Lists](categories-and-lists)

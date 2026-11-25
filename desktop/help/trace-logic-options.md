# Trace Logic Options

Sets how far the Trace Logic pane follows the logic from the selected activity. Trace Logic draws the selected activity as a box with its predecessors (activities that come before it) to the left and its successors (activities that follow it) to the right. Right-click the Trace Logic pane and choose **Trace Logic Options...**, or choose `View > Bottom Layout Options...` while Trace Logic is in the bottom pane. The Activity Network right-click menu has the same item.

## What is on it
- **Number of predecessor levels**: how many relationship steps back to show. 1 shows only the activities linked straight to the selected one; 2 adds their predecessors, and so on. 0 shows none.
- **Number of successor levels**: the same, going forward.
- **OK**: applies and closes. **Apply**: applies and stays open. **Cancel**: closes without changes. **Help**: opens this topic.

Each count can go from 0 to 2000. Both start at 3.

## Common tasks
1. Show Trace Logic in the bottom pane with `View > Show on Bottom > Trace Logic`.
2. Select an activity in the table. The pane shows its logic.
3. Right-click the pane and choose **Trace Logic Options...**.
4. Set **Number of predecessor levels** to 5 to follow the chain further back, and **Number of successor levels** to 0 to hide what follows.
5. Click **OK**.

## Reading the trace
- Each box is one activity. Boxes are placed in columns so a chain reads left to right.
- A straight line between side-by-side boxes is a finish to start link. Other links run below the boxes and rise into the successor with an arrow.
- Use **Zoom In**, **Zoom Out** and **Zoom to Best Fit** on the right-click menu to see more or less.

## Good to know
> If no activity is selected, the pane says "Select an activity to trace its logic."
> A large number of levels on a big schedule makes a wide diagram. Zoom to Best Fit helps.
> The level counts belong to the current layout and are kept in it when you save it.

## See also
- [Activity Network Options](activity-network-options)
- [Assign Predecessors and Successors](assign-relationships)
- [Edit Relationship](edit-relationship)

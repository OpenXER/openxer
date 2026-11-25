# Copy Activity Options

Chooses what comes along when you paste copied activities. It opens when you paste activities with `Edit > Paste` or `Ctrl+V` after copying or cutting them in the activity table.

## What is on it
Under **Specify the Activity information to be copied**:
- **Relationships**: copies the logic links of the activities.
  - **Only between copied activities**: keeps only the links where both ends were copied. Clear it to also keep links to activities that were not copied. Available only while Relationships is ticked.
- **Resource & Role Assignments**: copies the people, equipment and roles assigned to each activity.
- **Activity Codes**: copies the code values, such as area or crew, given to each activity.
- **Notebooks**: copies the notes attached to each activity.
- **Steps**: copies the step lists that break each activity into smaller pieces of work.
- **Expenses**: copies the costs entered against each activity.
- **Documents**: copies the document links of each activity.
- **Risks**: copies the risk links of each activity.
- **Skip this dialog next time**: pastes without asking from now on.
- **OK**: pastes the activities with the ticked items.
- **Cancel**: closes without pasting.
- **Help**: opens this topic.

Everything is ticked the first time. Your choices are kept for the next paste when you click **OK**.

## Common tasks
1. Select the activities to copy and choose `Edit > Copy`.
2. Select an activity in the WBS where the copies should go.
3. Choose `Edit > Paste`.
4. Clear the items you do not want, for example **Expenses**.
5. Click **OK**. The new activities are selected.

## Good to know
> The copies go into the WBS of the selected activity. With no activity selected, they go into the WBS you are filtered to, or the first one.
> A copy keeps its Activity ID when that ID is free in the project. Otherwise it gets a new one from the project's automatic numbering settings.
> After **Skip this dialog next time** is ticked, pasting uses the choices you made last time without showing this window. To see it again, tick **Show Copy Activity Options when pasting activities** in `Edit > User Preferences...`, **Application** tab.
> The whole paste is one undo step.

## See also
- [Renumber Activity IDs](renumber-activity-ids)
- [Activities view](view-activities)
- [User Preferences](user-preferences)

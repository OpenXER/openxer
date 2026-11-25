# Edit Relationship

Changes the type or lag of one relationship, or deletes it. A relationship is a logic link that says one activity (the predecessor) controls when another (the successor) can start or finish. Double-click or right-click a relationship line in the Gantt chart, or double-click one in the Activity Network.

## What is on it
- **Picture**: a small drawing of how the two bars connect for the chosen type.
- **Predecessor and successor**: the two activities, shown as "ID - Name", predecessor first.
- **Relationship Type**: one of four:
  - **FS** (finish to start): the successor starts after the predecessor finishes. This is the usual type.
  - **SS** (start to start): the successor starts after the predecessor starts.
  - **FF** (finish to finish): the successor finishes after the predecessor finishes.
  - **SF** (start to finish): the successor finishes after the predecessor starts.
- **Lag**: a wait added to the link, typed like a duration, such as 2d or 4h. A number without a unit is in your duration unit. A negative number overlaps the two activities.
- **OK**: saves the type and lag. `Enter` does the same.
- **Cancel**: closes without changing anything.
- **Delete**: removes the relationship, after asking (`Enter` answers **Yes**).
- **Help**: opens this topic.

## Common tasks
1. Double-click the relationship line in the Gantt chart.
2. Pick the new **Relationship Type**, for example **SS**.
3. Type a **Lag**, for example 2d, to wait two days after the predecessor starts.
4. Click **OK**, then press `F9` to schedule.

## Good to know
> If the lag cannot be read, or the two activities already have a link of that type, the dialog stays open and says why under **Lag**.
> If the change leaves an activity that already has progress out of sequence (started before its logic allows), OpenXER asks whether to keep it, when the out-of-sequence warning is on (User Preferences, Assistance tab).
> OK and Delete are one undo step each. After a delete, `Ctrl+Z` brings the relationship back.
> Links to activities in other open projects are edited in the Predecessors and Successors tabs, not here.

## See also
- [Assign Predecessors and Successors](assign-relationships)
- [Scheduling basics](scheduling-basics)
- [Trace Logic Options](trace-logic-options)

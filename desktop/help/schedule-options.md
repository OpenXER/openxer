# Schedule Options

Sets how the schedule is calculated for this project. Open it with the **Options...** button of the [Schedule](schedule) dialog (`F9`). The options are saved with the project and written to the XER, except **Schedule automatically when a change affects dates** and **Recalculate assignment costs after scheduling**, which are kept in the workspace.

## What is on it
On the **General** tab:
- **Ignore relationships to and from other projects**: when on, each project is scheduled on its own, and the dates that relationships to activities in other projects impose (early starts from outside predecessors, late finishes from outside successors) are not used. When off, open projects linked to each other are scheduled together as one network, and for a project that is not open the dates stored in the file still hold. With several projects open, the Default Project's choice applies to all of them.
- **Make open-ended activities critical**: an activity with no successor takes its early finish as its late finish, so it has zero float.
- **Use Expected Finish Dates**: an activity with an expected finish gets a remaining duration that runs to that date.
- **Schedule automatically when a change affects dates**: reschedules after every edit that changes dates.
- **Level resources during scheduling**: every run also levels resources.
- **Recalculate assignment costs after scheduling**: reprices assignments after each run.
- **When scheduling progressed activities use**: what to do when an activity started before its predecessor finished. **Retained Logic** keeps the remaining work waiting for the predecessor. **Progress Override** ignores the unfinished predecessor and lets the remaining work continue. **Actual Dates** schedules from the actual dates as recorded.
- **Calculate start-to-start lag from**: where the lag of a start-to-start tie from a started predecessor is measured, **Early Start** or **Actual Start**. Either way the part of the lag the predecessor has already worked is used up, so early and late dates come out the same. The choice changes the free float and driving flag of those ties under **Retained Logic** (and so the longest path), and where leveling places their successors.
- **Define critical activities as**: **Total Float less than or equal to** a value you type, or **Longest Path** (the chain of driving links that sets the project finish).
- **Calculate float based on finish date of**: **Each project** is used; **Opened projects** is not available.
- **Compute Total Float as**: start float, finish float, or the smaller. Total float is how long an activity can slip before it delays the finish.
- **Calendar for scheduling Relationship Lag**: which calendar counts lag: the predecessor's, the successor's, a 24 hour calendar or the project default calendar.

On the **Advanced** tab:
- **Calculate multiple float paths**: finds the chains of activities that lead into the activity picked in **Display multiple float paths ending with activity**, ranked by **Total Float** or **Free Float**. **Specify the number of paths to calculate** sets how many, from 1 to 1000.

## Common tasks
1. Press `F9` and click **Options...**.
2. Change the options, click **Close**, answer **Yes** to save them, then click **Schedule**.

## Good to know
> **Close**, **Cancel** and the window's close box all ask whether to save what you changed: **Yes** saves it, **No** drops it, **Cancel** goes back to the options. When every value is as it was when the window opened, they just close. `Escape` does nothing on this window.
> **Default** first asks whether to reset every option (`Enter` answers **Yes**). **Yes** puts the standard set in the window: Retained Logic, lag from Early Start, Expected Finish on, Finish Float, critical at Total Float of 0 or less, each project's own finish date for float, Predecessor Activity Calendar for lag, relationships to other projects and open-ended criticality off, leveling off, multiple float paths off (by Total Float, no ending activity, 10 paths). It leaves the two workspace check boxes (scheduling automatically, recalculating assignment costs) as they are. Nothing is saved until you close the window and answer **Yes**.
> Multiple float paths never change dates.

## See also
- [Schedule](schedule)
- [Level Resources](level-resources)
- [Scheduling basics](scheduling-basics)

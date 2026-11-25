# Schedule

Runs the schedule: OpenXER works out the early and late dates, float and critical path of every activity from its durations, relationships, constraints and progress. Open it with `Tools > Schedule...` or press `F9`.

## What is on it
- **Project(s) to schedule**: how many projects are open and will be scheduled together. You cannot type here.
- **All Projects use their own data date**: each project keeps the data date it has. The data date is the date progress is measured up to; no remaining work is scheduled before it. This is chosen each time the dialog opens.
- **Apply selected data date to all open projects**: type a new data date in the box beside it, or pick one from the calendar on the **...** button. The box starts on the project's current data date. Any date is accepted, earlier than the current one included. That date is used for every open project.
- **Project Forecast Start Date**: the project's forecast start, shown for reference.
- **Set Data Date and Planned Start to Project Forecast Start during scheduling**: moves both the data date and the planned start to the forecast start before the run, in place of the data date choice above. It is greyed out when the project has no forecast start, and starts unticked each time.
- **Log to file**: also writes the run's log to the file named below, `SCHEDLOG.TXT` in your Documents folder unless you pick another with **...**. **As Html** writes it as a web page instead of plain text. **Overwrite existing** replaces the file; **Append to existing** adds to the end.
- **Schedule**: runs the schedule and, when the run succeeds, closes the dialog. While scheduling runs, a progress window shows the current step, elapsed time and a **Cancel** button (or press `Escape`) to stop the run without changing the project.
- **View Log**: opens the Schedule Log window.
- **Options...**: opens [Schedule Options](schedule-options).
- **Cancel**: closes without scheduling.

## The Schedule Log window
It shows the report of the last run in this session: the Default Project (the project whose options the run used), the settings used, including whether the project schedules automatically, statistics, warnings (such as activities without predecessors or successors, or actual dates after the data date), the data date, the earliest early start, the latest early finish and the critical activities. A leveling run also writes its report here, and so does Recalculate Assignment Costs when it skips assignments.

## Common tasks
1. Press `F9`.
2. To move the data date, choose **Apply selected data date to all open projects** and type the new date.
3. Click **Schedule**. The status bar reports how many unfinished activities are critical by the project's own definition in [Schedule Options](schedule-options) (**Define critical activities as**: total float at or below the hours set there, or on the longest path).

## Good to know
> With several projects open and their data dates not all the same, **Schedule** asks nothing: with **All Projects use their own data date** (the default) each project is scheduled from its own date and keeps it; with **Apply selected data date to all open projects** every open project takes the date in the box.
> If the network has a circular relationship (a loop of links), nothing is scheduled: the dialog closes and OpenXER shows the loop instead.
> If the log file cannot be written, the run still happens and the status bar says the log was disabled.
> If **Level resources during scheduling** is on in Schedule Options, every run also levels resources.
> After a successful run OpenXER saves the project to the workspace.

## See also
- [Schedule Options](schedule-options)
- [Check Schedule](check-schedule)
- [Level Resources](level-resources)
- [Scheduling basics](scheduling-basics)

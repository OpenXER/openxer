# Scheduling basics in OpenXER

A schedule is a list of activities joined by relationships. OpenXER works out the
earliest and latest dates each activity can happen, which activities have no room
to slip, and when the project finishes. This page covers the ideas you need to
read and steer that calculation.

## The data date
The data date is "today" for the schedule: work before it is history, work after
it is forecast. Remaining work is never scheduled before the data date. When you
update progress you move the data date forward to the end of the period you are
reporting. You set it in the Schedule dialog. See [Schedule](schedule).

## Scheduling with F9
1. Press `F9` (or `Tools > Schedule...`).
2. Check the data date, then click **Schedule**.
3. Dates, float and the critical path update. **View Log** shows what the run
   found, such as activities without predecessors.

How the calculation treats lags, out-of-sequence progress and float is set in
[Schedule Options](schedule-options).

## Relationships and float
- A relationship says one activity drives another. Finish to start is the usual
  kind; start to start, finish to finish and start to finish also exist. A lag
  adds waiting time between the two.
- Float is how much an activity can slip before it delays the project finish (or a
  date it must meet). Critical activities are drawn in red on the bar chart. By
  default an activity is critical when its total float is zero or less;
  [Schedule Options](schedule-options) can set another limit or use the longest
  path instead.
- Drag from one bar end to another in the chart to add a relationship. See
  [Edit Relationship](edit-relationship).

## Constraints and the date marks
A constraint pins an activity to a date, for example "start on or after" or
"finish on or before". Use them sparingly: logic should drive dates.
- A start or finish date that a constraint applies to shows a trailing `*` mark in
  the table, the Gantt bar labels and the Activity Network boxes, so you can see
  at a glance which dates are held. The `*` follows a space, like the `A` mark.
- An actual date (work that really started or finished) shows a trailing `A`
  instead, never both. Actual dates come from progress, not from the
  calculation.
- Both marks are for display only. You can type or paste a date with or without
  them.

## Leveling resources
Leveling delays activities so that a resource is not booked beyond what it has
available. Use `Tools > Level Resources...` (`Shift+F9`). See
[Level Resources](level-resources).

## Baselines
A baseline is a frozen copy of the schedule you compare against later.
1. Save a copy with `Project > Maintain Baselines...`. See
   [Maintain Baselines](maintain-baselines).
2. Choose which copy the bars and variance columns compare to with
   `Project > Assign Baselines...`. See [Assign Baselines](assign-baselines).

## Good to know
> Editing a date or duration does not move the activities around it until you
  schedule again. Press `F9` after a batch of changes, or turn on **Schedule
  automatically when a change affects dates** in Schedule Options.
> [Check Schedule](check-schedule) lists common logic problems such as open ends,
  constraints, long lags and negative float.

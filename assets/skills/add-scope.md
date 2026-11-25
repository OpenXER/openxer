---
id: add-scope
name: Add a fragment of work
description: Adds a change order or added scope as a small set of activities and ties it into the existing logic on a new file.
request: Add this change order's work to the schedule.
use_when: The user wants to add a change order, extra scope or a new piece of work to an existing schedule and tie it in.
kind: workflow
order: 240
examples:
- Add the new pipe rack to the schedule
- Put change order 12 into the schedule
- We picked up extra paving, add it
tools:
- openxer_load
- openxer_paths
- openxer_milestones
- openxer_apply
- openxer_schedule
- openxer_compare
- openxer_check_schedule
---
# Add a fragment of work

You add new work as a small set of activities (a fragment), tie it into the
existing logic where it really connects, and show the effect on a new file.

## Ground rules

- Every date and move comes from a tool result. Never estimate the effect.
- Plain words; activity names a crew understands. Never say where this
  method comes from, and never mention how this skill was written.
- Writes: dry run first, show the change list, write a NEW file the user
  names, show what changed after. The user's file is never overwritten.

## Steps

1. Settle the file, the project and a NEW output path.
2. Define the fragment with the user, one question at a time: the
   activities (demolition before installation in each discipline), their
   durations and calendar, which WBS element they belong to (or a new one),
   and the reference for the change (change order number) for the names
   or a notebook.
3. Tie it in. Read the neighbours first with `openxer_load` (the activities
   it touches). For each fragment activity, agree:
   - what in the existing schedule must happen before it (access, a
     foundation, a shutdown);
   - what in the existing schedule waits for it.
   Every fragment activity gets a predecessor and a successor. A fragment
   with no successor into the existing work cannot move anything: say so.
4. One `openxer_apply` batch: `add_wbs` if needed, `add_activity`,
   `set_activity` (`original_duration`, `calendar`), `relationship` adds
   with `comments` naming the change, optionally `set_notebook` with the
   change reference, and `{"op":"schedule"}` at the end. Dry run, show the
   list, then write to the new output.
5. Measure: `openxer_compare` the new file against the old; `openxer_paths`
   on the new file to see whether the fragment is now on the critical
   path; `openxer_check_schedule` for open ends you may have left.

## Ask

One question at a time: activities, durations, ties, the change reference.

## Do not

Do not tie to activities just to close an open end; tie only where the work
really depends. Do not change existing durations or ties unless asked.

## Answer

- The file written and the activities added.
- The finish and milestones before and after, from the comparison.
- Whether the new work is on the critical path.
- Anything left open.

---
id: change-interview
name: Change interview
description: Before changing a schedule, settles every open decision one question at a time, dry runs the change, then applies it to a new file.
request: Interview me about this change before you make it.
use_when: The user asks for a change to the schedule (logic, durations, constraints, progress, resequencing) and it leaves decisions open, or they ask to be asked first.
kind: workflow
order: 210
examples:
- Tie the pump set to the grout cure
- Pull the hydrotest forward a week
- Resequence the tie-ins so the outage is shorter
- Ask me about this change first
tools:
- openxer_load
- openxer_paths
- openxer_milestones
- openxer_apply
- openxer_set_activity
- openxer_relationship
- openxer_schedule
- openxer_compare
---
# Change interview

The user asked for a change. Before you make it, settle every decision it
depends on, so the change written is the change they meant. You ask; they
decide.

## Ground rules

- Every date, float and move you quote comes from a tool result. Never work
  one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Writes: dry run first, show the change list, write a NEW file the user
  names, show what changed after. The user's file is never overwritten.

## How to interview

- Read first: `openxer_load` for the activities involved (dates, float,
  calendar, ties), `openxer_milestones` and `openxer_paths` for what drives
  the dates near the change. Never ask what the file answers; state it
  ("A1040 Set pump P-101 is on the 5x10 calendar").
- List the open decisions for yourself: what the request leaves open that
  changes the result. Ask only those, one at a time, each with your
  recommendation first and one line why.
- Settle what others depend on first (which activities are tied before
  what lag the tie gets).
- Walk branch by branch (scope, sequence, durations, reason) and show where
  you are: "Sequence, decision 3 of about 7". After each branch, two to
  four lines on what is settled.
- When an option's consequence matters, dry run it first and put the
  effect in the option: "moves Mechanical complete 2 days later".
- From the second question on, the user may say "use your recommendations
  for the rest": take them and say which.

## How deep

- Fully specified and small: no questions; go straight to the dry run.
- Small: one or two questions (the value left open, the reason).
- Big (resequencing, many activities, a milestone moving): branch by
  branch.
- Progress (actual dates, remaining duration, percent, the data date) is
  the field's record: ask where the number came from before changing it.

## The reason

Every change needs its reason: what happened and who decided. Put it in the
relationship `comments`, or in the activity's notebook with the
`set_notebook` operation in the same batch.

## Make the change

1. Write the change as one `openxer_apply` batch (or a single edit tool).
   Run it with `dry_run` true. Show a short Decisions list and the change
   list from the result.
2. On a yes, run it again to a NEW output file the user names, with the
   operation `{"op":"schedule"}` at the end so dates follow.
3. `openxer_compare` the new file against the old one and report what
   moved: the finish, the milestones, the biggest moves.

## Do not

- Never widen the change into changes they did not ask for; mention one
  worth making after, and let them ask.
- Nothing is written before the user says yes to the dry run.

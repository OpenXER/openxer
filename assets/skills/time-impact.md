---
id: time-impact
name: Time impact of a change
description: Inserts a fragment for a change or delay into a copy of the current schedule, reschedules, and measures the effect on a milestone, step by step.
request: Measure the time impact of this change on the schedule.
use_when: The user wants to know how much a change, delay or added work will move a milestone or the finish, before it happens, with the steps documented.
kind: workflow
order: 290
examples:
- How much will the late steel delivery push the finish?
- What is the time impact of change order 7?
- Measure the effect of this owner delay
tools:
- openxer_inspect
- openxer_load
- openxer_milestones
- openxer_paths
- openxer_apply
- openxer_schedule
- openxer_compare
---
# Time impact of a change

You measure, looking forward, what a change or delay does to a milestone:
model it as a small fragment of activities, insert it into a copy of the
current schedule, recalculate, and compare with the unimpacted copy. You
document every step so someone else can repeat it.

This is a planning aid: it shows the calculated effect on the current plan.
It is not a claim, an entitlement finding or a legal analysis, and it says
nothing about who is responsible. Say so in the answer.

## Ground rules

- Every date and move comes from a tool result on calculated copies. Never
  estimate the effect yourself.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Every write goes to a NEW file; dry run first and show the change list.
  The user's file is never changed.

## Steps

1. Settle the current schedule (the latest update), the milestone or
   finish to measure, and an output folder.
2. **Unimpacted copy**: run `openxer_schedule` on the current file to a NEW
   file, with no option changes, so both copies are calculated the same
   way. Record its milestone date with `openxer_milestones`.
3. **Fragment**: with the user, one question at a time: the activities
   that represent the change or delay (for example "Wait for revised
   drawings", "Fabricate added supports", "Install added supports"), their
   durations and the basis for each, and where they tie in: what they
   follow and which existing activities they hold back. Record the source
   of each number (letter, quote, foreman).
4. **Impacted copy**: one `openxer_apply` batch on the unimpacted copy:
   `add_activity`, `set_activity` (`original_duration`, `calendar`),
   `relationship` adds with `comments` naming the change, and
   `{"op":"schedule"}` at the end. Dry run, show the list, then a NEW file.
5. **Measure**: `openxer_compare` with `input` the impacted copy and
   `reference` the unimpacted copy: the milestone move, the finish move, the
   biggest moves. `openxer_paths` on the impacted copy: is the fragment on
   the path to the milestone now?
6. If the milestone does not move, say why from the results (the fragment
   had float to the milestone).

## Ask

One question at a time: the milestone, each fragment activity and its
basis, each tie.

## Do not

Do not change existing durations or logic in the impacted copy other than
inserting the fragment. Do not assign responsibility.

## Answer

- One line: the calculated move of the milestone, and that this is a
  planning aid, not a claim.
- The steps: files used and written, the fragment (activities, durations,
  bases, ties).
- The result: milestone and finish before and after, from the comparison;
  whether the fragment now drives it.

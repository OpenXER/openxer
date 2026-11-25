---
id: update-review
name: Update review
description: What changed since the last update or the baseline: dates, progress, logic, durations, constraints, calendars, and why the key dates moved.
request: Review this update against the last one.
use_when: The user asks what changed since the last update or the baseline, to review an update, why the finish or a milestone moved, whether progress or plan changes moved it, or to compare with last period's file.
kind: check
order: 40
compare: baseline-or-file
examples:
- What changed since last week?
- Why did the finish move?
- Was the slip progress or changes to the plan?
- Review this update before it goes out
tools:
- openxer_inspect
- openxer_compare
- openxer_paths
- openxer_milestones
- openxer_check_schedule
---
# Update review

You tell the user what changed between the last accepted schedule (or the
baseline) and this one, and why the key dates moved, in words a
superintendent understands. You only read. The reference file is only read.

## Ground rules

- Every date, count, move and float comes from a tool result. Moves from
  `openxer_compare` are calendar days: say so.
- Plain words. Never say where this review method comes from, and never
  mention how this skill was written.

## Steps

1. Settle the reference, one question at a time: last period's file (an
   XER or workspace path the user gives) or a stored baseline of a
   workspace project (`baseline` `project` or `primary`). Never look for
   files yourself and never pass a path the user did not give.
2. Run `openxer_compare`. If it fails because the reference holds several
   projects, run `openxer_inspect` on it and ask which.
3. Lead with the dates: `project_finish` then and now, the data dates, and
   `milestones_moved`.
4. Explain why, in this order:
   - **Progress**: started and finished since; warning signs: actual dates
     rewritten, status gone back, remaining duration up on work in
     progress. These question whether the progress is credible.
   - **Logic changes**: ties added, removed or changed (type or lag). Each one can move
     dates; name the biggest by the activities they touch.
   - **Durations**: original durations changed.
   - **Constraints and calendars** changed.
   - **Activities** added, deleted, renamed.
5. Critical path movement: run `openxer_paths` (no activity) on the current
   file and, when the user gave a reference file, on that file too. Say
   whether path 1 is the same chain, and which activities joined or left
   it.
6. Float erosion: from `biggest_finish_moves`, name the activities whose
   float fell the most (`total_float_hours_then` and `_now`). Do not
   subtract them yourself; quote both values.
7. Run `openxer_check_schedule` on the current file and mention any new
   out-of-sequence progress or invalid dates.
8. Against a baseline, the comparison covers every period since it was set,
   not one update. Say so.

## Ask

One question at a time: which reference, which project in it.

## Do not

Do not say who caused a delay. This is a review of what changed, not a
delay claim. Do not change anything.

## Answer

- Lead: what moved and the main reason, for example "Finish unchanged;
  the permit milestones slipped 26 days, mostly from late progress."
- Dates: data dates, finish then and now, milestones that moved with
  Activity ID, name, dates and the move in calendar days.
- One section per kind of change with its count on the heading line and the
  top items: before and after.
- Critical path: same or changed, and float erosion.
- One line naming the kinds with no change.

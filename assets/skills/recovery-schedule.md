---
id: recovery-schedule
name: Recovery schedule
description: When a milestone is late, finds and ranks the levers that could recover it, measures each on a copy, and states its cost or risk.
request: This milestone is late; find me ways to recover it.
use_when: The user has a late milestone or finish and wants recovery options, a recovery plan, or to know how to get back on schedule.
kind: workflow
order: 280
examples:
- We are three weeks late on mechanical completion, how do we get it back?
- Build a recovery plan for the finish
- What can we do to recover the outage date?
tools:
- openxer_milestones
- openxer_paths
- openxer_load
- openxer_resource_loading
- openxer_apply
- openxer_schedule
- openxer_compare
- openxer_copy_calendar
- openxer_calendar_weekday
---
# Recovery schedule

A milestone is late. You find the levers that could pull it back, test each
on its own copy, rank them by what they recover against what they cost or
risk, and only then, if the user asks, combine the chosen ones into a
recovery schedule on a new file.

## Ground rules

- Every date, gap and move comes from a tool result on a calculated copy.
  Never estimate what a lever recovers.
- No lever is free. Each comes with its cost or risk: overtime and fatigue,
  more crews and supervision, stacking in tight areas, extra equipment,
  quality and safety exposure, risk pushed later. Say it every time.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Every option is a NEW file; dry run first and show the change list. The
  user's file is never changed.

## Steps

1. Settle the file, the late milestone (`openxer_milestones` to find it),
   the date it must make, and an output folder.
2. Read the problem: `openxer_paths` for the milestone with count 5: path
   1 is what must be shortened; `behind_path_1_hours` of path 2 caps what
   shortening path 1 alone can recover, because path 2 then drives. Say
   that cap plainly.
3. List candidate levers on path 1 (and on paths within reach), with the
   user, one question at a time:
   - Resequence: overlap steps (start to start with lag) where the field
     agrees it is safe.
   - Shorten durations: more crews or longer shifts on specific
     activities; check `openxer_resource_loading` for room.
   - Calendars: extra working days for specific work.
   - Remove or relax a constraint that is not a real date.
   - Re-scope: move work out of the milestone's scope, if the contract
     allows.
4. Test each lever alone: one `openxer_apply` batch with the change and
   `{"op":"schedule"}`, dry run, then a NEW file; `openxer_compare` against
   the current file; `openxer_paths` on the copy for the new driver.
5. Rank by days recovered for the milestone (from the comparison) against
   cost and risk. Show where path 2 takes over.
6. If the user picks levers, combine them in one new copy the same way,
   and measure again: combined levers rarely add up.

## Ask

One question at a time: the target date, which levers the field accepts,
crew and overtime limits.

## Do not

Do not present a recovery plan as achievable unless the combined copy's
calculated date meets the target. Do not change the user's file.

## Answer

- Two lines: how late, and the most the levers tested can recover.
- A table: lever, days recovered (from the tools), new driver, cost and
  risk.
- The combined option if built: its calculated milestone date and what it
  commits the job to.

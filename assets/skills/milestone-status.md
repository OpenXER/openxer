---
id: milestone-status
name: Milestone status
description: Each milestone's date, float, move since the baseline, and what drives it.
request: Give me the milestone status.
use_when: The user asks where the milestones stand, which milestones are at risk, what drives a milestone, or for a milestone report.
kind: check
order: 110
examples:
- Milestone status
- Which milestones are at risk?
- What is driving mechanical completion?
tools:
- openxer_inspect
- openxer_milestones
- openxer_paths
---
# Milestone status

You report each milestone: when it is forecast, how much room it has, how
far it moved against the baseline, and what drives it. You only read.

## Ground rules

- Every date, float and move comes from a tool result. Moves are calendar
  days. Never work one out yourself.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects).
2. Run `openxer_milestones`. Take `date`, `status`, `total_float_hours`,
   `constraints`, `baseline_date`, `moved_days_since_baseline`,
   `driven_by`.
3. Sort for the reader: open milestones first, by date; completed ones in
   one line at the end.
4. Flag at risk: negative float, zero float, moved later since the
   baseline, or held by a constraint that the logic does not support.
5. Driver: name the driving predecessor (`driven_by`). When it is empty and
   the milestone has predecessors, the date is held by a constraint or the
   data date; when it has no predecessors, nothing ties it in: say so. For
   the milestones the user cares most about, run `openxer_paths` with that
   activity to show the chain behind it.

## Ask

One question at a time: which milestones are contract or owner dates.

## Do not

Do not change anything. Do not call a milestone safe because it has float
if that float comes from a constraint or open end.

## Answer

- Two lines: how many open milestones, how many at risk, the worst one.
- A table or list: ID, name, date, float, move since baseline, driver.
- At risk: one line each with why.

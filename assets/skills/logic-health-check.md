---
id: logic-health-check
name: Logic health check
description: Checks the network: open ends, lags, relationship types, constraints, float, long durations, bad progress dates. Says what to fix first.
request: Run the logic health check on this schedule.
use_when: The user asks whether the schedule is healthy, to check the logic, for open ends or dangling activities, leads and lags, hard constraints, high or negative float, long durations, invalid dates or out-of-sequence progress.
kind: check
order: 10
examples:
- Check the logic
- Is this schedule healthy?
- Are there any open ends?
- Which activities have hard constraints?
- Run a 14-point style check
tools:
- openxer_inspect
- openxer_check_schedule
- openxer_paths
- openxer_load
---
# Logic health check

You are checking whether this schedule's network is sound enough to trust
its dates and float, and telling the user, in words a superintendent
understands, what is wrong and what to fix first. You only read. Nothing is
changed.

## Ground rules

- Every count, percent, threshold, date and float in your answer comes from
  a tool result. Never count, add up or work out a percent yourself. If a
  number you want is not in a result, say it is not available.
- Plain words. Never say where this check or its method comes from, and
  never mention how this skill was written. If the user asked for a
  well-known assessment by its own name, you may use that name back; call
  the limits "commonly used thresholds", not anyone's rule.
- The dates and float are the ones stored when the project was last
  scheduled. If the user says they changed it since, offer to schedule a
  copy first (`openxer_schedule` to a new file) and run the check on that.

## Steps

1. Settle the file. If the user has not given the path of the XER or
   OpenXER workspace, ask for it. For a workspace or a file with several
   projects, run `openxer_inspect` and ask which project, one question.
2. Run `openxer_check_schedule`. Take from it: `summary`, every check with
   `passed` false, its `count`, `out_of`, `value`, `target` and the items in
   `found`, and the `logic_warnings` lists.
3. Rank the failed checks by consequence, worst first, unless the numbers
   clearly say otherwise:
   - Invalid progress dates: the dates themselves cannot be trusted.
   - Negative float: a date the plan does not meet as it stands.
   - Missing logic (no predecessor or no successor): a slip there moves
     nothing, so the critical path and float are wrong.
   - Hard constraints: dates held by a constraint, not by the work.
   - Out-of-sequence progress: the field is working in a different order
     than the schedule says.
   - Negative lags, long lags, relationship types other than finish to
     start: logic that hides work or is hard to follow.
   - Large float and large durations: usually missing ties, or activities
     too big to status.
   - Activities without resources, late activities, the baseline execution
     index last (they need a baseline and loaded resources to mean much).
4. For each failed check say what it means on this job, the count against
   the target, and the worst items: Activity ID and full name as the result
   gives them, then the number that matters (float, lag, duration, date).
   For a tie, name predecessor then successor, with type and lag.
5. A constraint on a contract milestone, or a lag that stands for a real wait
   such as concrete cure, can be right. Ask the user about it rather than
   counting it as a defect when it looks deliberate.
6. Optional test of the critical path: run `openxer_paths` with no activity
   and say whether path 1 is held by a constraint or by an activity with
   nothing before it (`first_activity_held_by`).

## Ask

One question at a time, only what the tools cannot answer: which project,
whether a constraint or lag is deliberate, which unit (days or hours) the
user wants.

## Do not

- Do not change the schedule or propose a fix on your own. After the
  write-up you may ask whether the user wants fixes proposed for one finding.
- Do not call a check passed or failed differently from `passed`.
- Do not invent a threshold; quote the result's `target` and
  `threshold_hours`.

## Answer

- Two lines first: how many checks failed out of how many, and the one thing
  to fix first.
- One short section per failed check, worst first, with the result on the
  heading line, for example `Missing logic: 14 of 1,018 open activities
  (1.4%), target under 5%`.
- Under it one line on what it means here, then the worst items, one per
  line. If the list was cut, say how many more (`more`).
- One line listing the checks that passed.
- A last short section: the thresholds used, that dates are as last
  scheduled, and what the check does not cover (`not_checked`).

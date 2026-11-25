---
id: float-constraint-review
name: Float and constraint review
description: Negative float, hard constraints, and where float is held back artificially by constraints, lags, open ends or a must finish date.
request: Review the float and constraints.
use_when: The user asks about negative float, constraints, why float looks too high or too low, whether float is being held or hidden, or which dates are forced.
kind: check
order: 60
examples:
- Why do we have negative float?
- Which dates are forced by constraints?
- Is float being held back somewhere?
tools:
- openxer_inspect
- openxer_check_schedule
- openxer_paths
- openxer_milestones
---
# Float and constraint review

You find where float is not telling the truth: negative float the plan does
not meet, dates held by constraints instead of work, and float that is
inflated or hidden. You only read.

## Ground rules

- Every float, count and date comes from a tool result. Never work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written. Call limits "commonly used thresholds".

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects).
2. Run `openxer_check_schedule`. Take the Negative Float, Hard Constraints,
   Soft Constraints, Large Float, Logic and Long Lags checks and the
   `logic_warnings` lists `constrained`, `without_successors`.
3. Run `openxer_milestones` and `openxer_paths` (no activity). Take the
   must finish date from `basis.must_finish_by` beside the scheduled
   finish.
4. Read the findings in this order:
   - **Negative float**: which activities, how much, and what they drive.
     Negative float means a constraint or must finish date the logic does
     not meet. Name the constraint or date that causes it when the result
     shows one (milestones with constraints, or the must finish date).
   - **Hard constraints**: each one holds a date regardless of the work
     before it. List them with type and date. Ask whether each is a real
     contract or access date; one that is not should be removed so the
     logic drives the date.
   - **Constraints at the head of a path** (`first_activity_held_by`): the
     path's dates come from the constraint.
   - **Float held back or inflated**:
     - a must finish date later than the scheduled finish gives every
       activity extra float; earlier takes it away;
     - activities without successors carry float up to the project end
       that is not real;
     - long lags can stand in for work and hide float;
     - "as late as possible" constraints use up float on purpose;
     - large float lists often point at missing ties.
5. Rank by consequence: negative float on milestones first, then hard
   constraints on or near path 1, then inflated float on work the field
   thinks is urgent, then the rest.

## Ask

One question at a time: whether a constraint or lag is deliberate and who
set it.

## Do not

Do not remove constraints or change dates. After the write-up you may offer
to dry-run removing one constraint on a copy (`openxer_set_activity` with
`dry_run`) so the user sees the effect.

## Answer

- Two lines: the biggest float problem and its cause.
- Negative float: activities and the cause.
- Hard constraints: one per line with type, date, and your question.
- Float held back or inflated: one line per cause with the evidence.
- What to fix first.

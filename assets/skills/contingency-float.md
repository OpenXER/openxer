---
id: contingency-float
name: Contingency and float ownership read
description: Where the schedule's time buffers sit, how big they are, and whether they are visible or hidden.
request: Show me where the time buffers are in this schedule.
use_when: The user asks where contingency or buffer time is, who owns the float, whether there is hidden time, or how much room the finish really has.
kind: check
order: 190
examples:
- Where is the contingency?
- How much room does the finish really have?
- Is anyone hiding float?
tools:
- openxer_inspect
- openxer_milestones
- openxer_paths
- openxer_check_schedule
- openxer_load
---
# Contingency and float ownership read

You find where spare time sits in the schedule: visible buffers, float
before milestones, and time hidden in constraints, lags, durations or a
late must finish date, so the user knows what room they really have. You
only read.

## Ground rules

- Every float, date and count comes from a tool result. Never work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Who owns float is a contract question. Ask what the contract says; do not
  rule on it.

## Steps

1. Settle the file and project (`openxer_inspect` for several). Ask what
   the contract says about float and contingency, one question.
2. Gather: `openxer_milestones` (float on each milestone),
   `openxer_paths` (no activity; path float and gaps), and
   `openxer_check_schedule` (Long Lags, Large Float, Hard Constraints,
   `without_successors`). Take the must finish date from `basis`.
3. Find the buffers:
   - **Visible**: activities named contingency, buffer, allowance or float
     (search `openxer_load` names). Their duration and where they sit
     (before which milestone).
   - **Project-level**: a must finish date later than the scheduled finish
     gives every activity the same extra float. Report the two dates side
     by side.
   - **Milestone float**: milestones with positive float to their own
     constraint.
   - **Hidden**: long lags standing for work, constraints that hold work
     later than needed (as late as possible, start on or after), durations
     the user says are padded, open ends with large float.
4. Say which buffers protect the finish and which only look like room
   (float on work with no successor protects nothing).

## Ask

One question at a time: the contract's float rule, whether a lag or a
long duration contains a buffer.

## Do not

Do not move or remove a buffer. Do not decide who owns float.

## Answer

- Two lines: the real room before the finish (from path float and
  milestone float) and where it sits.
- Visible buffers, project-level float, milestone float, hidden time, each
  with the numbers from the tools.
- What to make visible.

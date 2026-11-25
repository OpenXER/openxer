---
id: baseline-review
name: Baseline schedule review
description: Before a first baseline is accepted: complete scope, sound logic, contract dates in the structure, a believable critical path.
request: Review this baseline schedule before we accept it.
use_when: The user is about to accept, approve or submit a first baseline schedule, or asks whether a baseline is fit to be accepted.
kind: check
order: 160
examples:
- Is this baseline ready to approve?
- Review the contractor's baseline submittal
- Would you accept this baseline?
tools:
- openxer_inspect
- openxer_check_schedule
- openxer_paths
- openxer_milestones
- openxer_schedule_stats
- openxer_wbs_summary
- openxer_resource_loading
---
# Baseline schedule review

A baseline becomes the yardstick for every update after it, so it is held
to a stricter standard than an update. You check that it is complete,
logical, built around the contract's dates, and has a critical path a
superintendent would believe. You only read.

## Ground rules

- Every count, date and float comes from a tool result. Never work one out.
- Plain words. Never say where this review method comes from, and never
  mention how this skill was written.

## Steps

1. Settle the file and project (`openxer_inspect` for several). Ask for
   the contract milestones and finish date, and what the contract asks the
   schedule to show (resources, costs, a level of detail), one question at
   a time.
2. Gather: `openxer_check_schedule`, `openxer_paths`, `openxer_milestones`,
   `openxer_schedule_stats`, `openxer_wbs_summary` (level 2), and
   `openxer_resource_loading` when resources are required.
3. Judge, in this order:
   - **Scope**: does the WBS cover every area and phase of the contract?
     Ask about any that seem missing. Elements with no activities are a
     sign.
   - **Contract dates**: each contract milestone exists, is tied into the
     work, and is met (zero or positive float). A constraint on a contract
     milestone is acceptable; constraints elsewhere need a reason.
   - **Logic**: in a baseline, open ends, hard constraints holding work,
     negative float and invalid dates are reasons to send it back.
   - **Critical path**: from logic, through real work, starting at the
     start. Not held by constraints or long lags.
   - **Detail**: activities sized for statusing; names a crew understands.
   - **Resources and calendars**: when required, loaded and realistic.
4. Sort every finding into "must be corrected before acceptance" or
   "comment", with why.

## Ask

One question at a time: contract milestones, contract requirements, whether
a constraint is a contract date.

## Do not

Do not accept a baseline with negative float or open ends on the critical
path as "fine with comments". Do not change anything.

## Answer

- Two lines: accept, accept with comments, or return for correction, and
  the main reason.
- Must be corrected: worst first, evidence and why.
- Comments.
- What looks sound.

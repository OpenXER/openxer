---
id: baseline-variance
name: Baseline variance
description: Slips against the baseline ranked by consequence: milestones, the finish, and the work that moved most.
request: Show me the slips against the baseline.
use_when: The user asks what slipped against the baseline, how far behind plan the job is, which milestones are late against plan, or for a variance list.
kind: check
order: 70
examples:
- How far behind the baseline are we?
- What slipped against plan?
- Variance report
tools:
- openxer_inspect
- openxer_compare
- openxer_milestones
- openxer_paths
- openxer_wbs_summary
---
# Baseline variance

You show what has slipped against the approved plan, ranked by what it does
to the dates that matter, in plain words. You only read.

## Ground rules

- Every date, move and count comes from a tool result. Moves from
  `openxer_compare` and `openxer_milestones` are calendar days: say so.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects).
2. Settle the baseline. For a workspace project, `openxer_compare` with
   `baseline` `project` (or `primary` if the user says so). If there is
   none, ask whether they have the baseline as a separate file, and use
   `reference`.
3. Run `openxer_compare`, `openxer_milestones` and `openxer_wbs_summary`
   (level 2, deeper if the user wants).
4. Rank by consequence:
   - The project finish move (`project_finish.moved_days`).
   - Milestones that moved, contract milestones first, then by size.
   - Activities on the current critical path that moved: run
     `openxer_paths` and match its path 1 activities to
     `biggest_finish_moves`.
   - The other biggest moves.
   - Areas (WBS) whose finish moved most (`finish_moved_days_since_baseline`).
5. For each item give a likely reason only when the comparison shows one
   (a duration grew, a tie was added, it started late). Otherwise say the
   comparison does not show why and suggest the update review.

## Ask

One question at a time: which baseline, which milestones are contract
dates.

## Do not

Do not blame anyone for a slip. Do not change anything.

## Answer

- Two lines: finish then and now with the move, and the milestone most
  behind.
- Milestones: one per line, ID, name, baseline date, current date, move.
- Critical work that slipped.
- Areas behind most.
- What to look into next.

---
id: levels-of-detail
name: Levels of detail check
description: Checks that summary, intermediate and detailed levels of the schedule roll up consistently and are filed in the right place.
request: Check that the schedule levels roll up consistently.
use_when: The user asks whether the summary and detailed levels agree, whether the WBS rolls up properly, whether activities are filed in the right place, or about level 1, 2 and 3 schedules.
kind: check
order: 198
examples:
- Do the summary dates match the detail?
- Is the WBS rolled up properly?
- Are activities filed under the right areas?
tools:
- openxer_inspect
- openxer_wbs_summary
- openxer_schedule_stats
- openxer_milestones
- openxer_load
---
# Levels of detail check

You check that the schedule's levels agree: each summary level's dates span
the work below it, milestones at summary level match the detailed work that
achieves them, and activities are filed under the element they belong to.
You only read.

## Ground rules

- Every date, count and float comes from a tool result. Never work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (`openxer_inspect` for several).
2. Run `openxer_schedule_stats`: `wbs.activities_by_level` (how deep the
   work is filed), `elements_with_no_activities`,
   `element_with_most_activities`, `by_type` (Level of Effort and WBS
   Summary activities).
3. Run `openxer_wbs_summary` at level 1, then at the deepest level that
   holds activities (from step 2, up to 4).
4. Check:
   - **Filing depth**: activities filed directly on a top-level element
     while their siblings sit deeper are often misfiled.
   - **Empty elements**: summary elements with nothing under them are
     placeholders or missing scope; ask which.
   - **Overloaded elements**: one element holding far more activities
     than the rest may need splitting.
   - **Dates**: a summary element's dates come from its activities; an
     element whose finish is far beyond its siblings' is worth a look.
   - **Summary activities**: Level of Effort and WBS Summary activities
     should span the work they summarize; read them with `openxer_load`
     and compare their dates with the element's rollup.
   - **Milestones**: each key milestone (`openxer_milestones`) sits under
     the element whose work achieves it and is tied to that work.
5. Rank by consequence: misfiled or untied work that changes a reported
   summary date first.

## Ask

One question at a time: what each level is for, whether an empty element
is intended.

## Do not

Do not move activities or change the WBS. Offer the "Change interview"
skill for changes.

## Answer

- Two lines: whether the levels agree and the biggest mismatch.
- Filing, empty and overloaded elements, date mismatches, summary
  activities, milestones: each with examples by ID or WBS code.

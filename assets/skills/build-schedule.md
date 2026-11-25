---
id: build-schedule
name: Build a schedule from a scope
description: A staged interview from scope to a calculated schedule: purpose, milestones, areas, activities, logic, durations, calendars, then build, schedule and check.
request: Help me build a schedule from this scope.
use_when: The user wants a new schedule built from a scope of work, a bid, a list of work or a description of a job.
kind: workflow
order: 200
examples:
- Build me a schedule for this scope
- Turn this scope into a schedule
- Start a new schedule for the pump station job
tools:
- openxer_inspect
- openxer_eps_list
- openxer_eps_edit
- openxer_apply
- openxer_schedule
- openxer_check_schedule
- openxer_paths
- openxer_milestones
- openxer_load
---
# Build a schedule from a scope

You build a new schedule with the user, stage by stage, then create it,
calculate it, check it and say what is still weak. You ask; they decide.

## Ground rules

- Never state a date, a duration total or a finish before the schedule has
  been calculated by `openxer_schedule` (or the `schedule` operation). Every
  date and float you report comes from a tool result.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Activities are named so a crew understands them: verb and object and
  place ("Set pump P-101", "Pour slab on grade area B"). No vague names.
- Writes: dry run first, show the change list, write a NEW file the user
  names, show what changed after. The user's file is never overwritten.

## The interview (one question at a time, with your recommendation)

Show where you are: "Stage 3 of 8, question 2".

1. **Purpose and detail**: what the schedule is for (bid, baseline, short
   job plan), who uses it, the update cycle, and how fine activities should
   be (a useful rule: an activity a crew can status in one or two update
   periods).
2. **Milestones**: start, contract and owner dates, access dates, the
   finish. Which are fixed dates and which simply follow the work.
3. **Areas and WBS**: break the job by area or system, then by phase.
   Propose a WBS with codes (1, 1.1, 1.2) and confirm it.
4. **Activities per area**: list them, demolition before installation in
   each discipline, receive before set, set before connect. Confirm area by
   area. Summarize each area in two to four lines before the next.
5. **Logic before durations**: for each activity, what must finish (or
   start) before it. Ask about the tricky ones only: shared cranes, outages,
   inspections, cure, deliveries. Every activity gets a predecessor and a
   successor, except the first and last milestones.
6. **Durations**: in working days or hours, from the user's crew rates or
   experience. Ask for the basis of the big ones. Durations are inputs, not
   results: fine to state what the user gave.
7. **Calendars**: how the crews work (days a week, hours a day), holidays,
   any 7-day work (cure, deliveries).
8. **Resources** (only if wanted): crews or trades and their size; the
   "Resource-load a schedule" skill does this in detail.

The user may say "use your recommendations for the rest": then take them
and list what you took.

## Build it

1. Ask for the path of the user's OpenXER workspace (the openxer.db the app
   uses, or a copy) and for a NEW output path. Run `openxer_eps_list` to
   pick the folder (EPS ID).
2. `openxer_eps_edit` op `create_project` (folder, project_id, name,
   planned_start, must_finish when there is a contract finish) with
   `dry_run` first, then to the new output.
3. `openxer_inspect` on the new file to get the new project's
   `workspace_id`.
4. One `openxer_apply` batch on it with `dry_run` true: `add_wbs` for each
   element, `copy_calendar` and `calendar_weekday` and `calendar_day` for
   calendars, `add_activity` (activity, name, wbs), `set_activity` for
   `original_duration` (hours), `calendar`, `activity_type` for milestones,
   `primary_constraint` only for fixed dates, and `relationship` (action
   add, predecessor, successor, kind, lag_hours, comments saying why).
   Show the user the change list; fix what they question.
5. Apply the same batch to a NEW output file, ending with the operation
   `{"op":"schedule"}`.

## Check it

1. `openxer_check_schedule`, `openxer_paths` and `openxer_milestones` on
   the result.
2. Now, and only now, give the dates: the finish and the milestones from
   the results.
3. Report what is still weak: open ends, constraints, long durations, the
   critical path and whether it makes sense in the field, milestones that
   miss a contract date.

## Do not

- Do not fill gaps with invented durations or logic without saying they
  are your recommendation and getting a yes.
- Do not add owner-supplied work, shop work or anything the user says is
  out of scope.

## Answer at the end

- The file written, the project and how many activities, ties, milestones.
- The finish and milestone dates from the calculation.
- What is still weak, worst first, and what to settle next.

---
id: schedule-basis
name: Schedule design and basis
description: Settles purpose, users, detail, WBS, coding, calendars and update cycle before building, and writes the schedule basis document.
request: Help me set up the schedule basis.
use_when: The user is about to build a schedule and wants to plan its design, or needs a written basis of schedule (scope, assumptions, exclusions, calendars, constraints, critical path, risks).
kind: workflow
order: 270
examples:
- Write the basis of schedule
- Before we build it, how should this schedule be set up?
- Document the schedule assumptions
tools:
- openxer_inspect
- openxer_schedule_stats
- openxer_milestones
- openxer_paths
- openxer_check_schedule
- openxer_apply
---
# Schedule design and basis

Two parts: before building, settle how the schedule will be set up; once a
schedule exists, write down the basis it rests on, so anyone reading it
knows what it covers and what it assumes.

## Ground rules

- Every count, date and float in the basis comes from a tool result. Never
  work one out; the design decisions are the user's words.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- The only write is an optional notebook entry: dry run first, a NEW file,
  never overwrite.

## Part 1: design (one question at a time, with your recommendation)

1. Purpose: bid, baseline for approval, execution plan, recovery plan.
2. Users: owner, project manager, superintendents, foremen; what each needs
   from it.
3. Level of detail: summary, control or field level; a useful rule is an
   activity a crew can status within one or two update periods.
4. WBS: by area, system or phase; how deep.
5. Coding: the activity codes the users will filter by (area, discipline,
   responsibility, phase). Keep the list short.
6. Calendars: crew workweeks, holidays, 7-day items, weather time.
7. Update cycle: weekly or monthly, data date day, who reports progress.
8. Key dates: contract milestones, access and outage windows.

Write the decisions as a short design note in your answer.

## Part 2: the basis (once the schedule exists)

1. Settle the file and project (`openxer_inspect` for several).
2. Gather: `openxer_schedule_stats` (activities, calendars, durations,
   WBS), `openxer_milestones` (dates, constraints), `openxer_paths` (what
   drives the finish), `openxer_check_schedule` (open ends, constraints).
3. Ask the user, one question at a time, what the tools cannot know: scope
   included, scope excluded (owner-supplied items, shop work, work by
   others), assumptions (productivity, access, deliveries, permits), and
   the risks they see.
4. Write the basis with these sections: purpose and use; scope covered;
   exclusions; assumptions; calendars (from the stats); key dates and
   constraints (from the milestones, each with why it is held); critical
   path summary (from the paths, in field words); known risks; update
   cycle.
5. If the user wants it kept in the schedule, add it as a notebook on the
   first milestone with `openxer_apply` `set_notebook` (dry run, then a NEW
   file).

## Do not

Do not invent assumptions or exclusions; ask. Do not state a date the
tools did not return.

## Answer

The design note (part 1) or the basis document (part 2), in the sections
above, short enough to read in five minutes.

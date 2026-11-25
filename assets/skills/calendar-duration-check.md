---
id: calendar-duration-check
name: Calendar and duration sanity
description: Activities on the wrong calendar, odd workweeks, very long or zero durations, finish milestones fed from another calendar.
request: Check the calendars and durations.
use_when: The user asks whether calendars are right, about odd workweeks or holidays, activities on the wrong calendar, or very long or zero durations.
kind: check
order: 100
examples:
- Are the calendars right?
- Any activities with silly durations?
- Which activities are on the 7-day calendar?
tools:
- openxer_inspect
- openxer_schedule_stats
- openxer_check_schedule
- openxer_load
---
# Calendar and duration sanity

You check that each activity works on a calendar that matches how the work
is done, that the calendars themselves look like real workweeks, and that
durations are neither absurdly long nor zero where work is real. You only
read.

## Ground rules

- Every count, hour and duration comes from a tool result. Never work one
  out yourself.
- Plain words. Never say where this check comes from, and never mention how
  this skill was written.

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects).
2. Run `openxer_schedule_stats`. Take `calendars`, `durations`, `by_type`.
3. Calendars:
   - For each calendar with activities: hours per week, working days per
     week, hours per working day, nonwork exception dates and their range.
   - Odd workweeks: zero working days, more than 7 days, a working day of
     over 12 hours, no holidays at all across a long job, exceptions that
     stop long before the job ends.
   - A continuous (every day, 24 hours) calendar suits curing, deliveries
     and reviews, not crews. Ask the user which activities on it are crew
     work; `openxer_load` lists each activity's calendar.
4. Durations (working days on each activity's own calendar):
   - The spread and the longest activities. Very long ones are hard to
     status and hide sequence.
   - Zero-duration tasks that are not milestones: either a milestone in
     disguise or missing work.
5. Run `openxer_check_schedule`; take
   `finish_milestones_fed_from_another_calendar`: a milestone on a 5-day
   calendar fed by 7-day work can land on a day nobody works.
6. Rank by consequence: crew work on the wrong calendar on the critical
   path, then calendars with impossible weeks, then long durations on
   critical work, then zero durations.

## Ask

One question at a time: how the crews really work (days and hours), which
holidays the job observes.

## Do not

Do not change calendars or durations. Offer a fix only after the write-up,
one finding at a time.

## Answer

- Two lines: the biggest calendar or duration problem.
- Calendars: one line each with hours and days, and anything odd.
- Activities on a calendar that looks wrong.
- Durations: spread, longest, zero.

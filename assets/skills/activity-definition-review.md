---
id: activity-definition-review
name: Activity definition review
description: Are activities single-scope, single-crew, measurable, sized for the update cycle, and named so a crew understands them?
request: Review how the activities are defined.
use_when: The user asks whether activities are well defined, too big or too small, named clearly, measurable, or fit for statusing by the field.
kind: check
order: 140
examples:
- Are these activities any good?
- Can the foremen status these activities?
- Which activities are too big?
tools:
- openxer_inspect
- openxer_schedule_stats
- openxer_load
---
# Activity definition review

You check that each activity is one piece of work, by one crew or
responsible party, with a clear finish, small enough to status, and named
so a foreman knows what it is. You only read.

## Ground rules

- Every count and duration comes from a tool result. Never work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (`openxer_inspect` for several), and ask
   the update cycle (weekly or monthly) in working days.
2. Run `openxer_schedule_stats` with `update_period_days`. Take
   `durations` (spread, longest, zero-duration tasks,
   `longer_than_two_update_periods`), `names` (repeated, one-word),
   `open_work_without_resources_or_expenses`.
3. Run `openxer_load` (page through with offset when there are many) to
   read the names of the activities, then judge each kind of problem with
   examples by Activity ID:
   - **Too big**: longer than two update periods; hard to status, hides
     sequence. Suggest how to split (by area, by step).
   - **Too vague**: names like "Piping", "Misc work", "Electrical"; one-word
     names; repeated names without the area that tells them apart.
   - **Mixed scope**: names joining different work or different crews
     ("Install and test", "Civil and mechanical").
   - **No clear finish**: names without an end point ("Support
     construction"). An activity needs a way to say it is done.
   - **Responsibility**: when the job uses codes or resources for who does
     it, work with none.
4. Rank by consequence: big or vague activities on the critical path first,
   then the rest by size.

## Ask

One question at a time: the update cycle, how the field statuses
(remaining days or percent).

## Do not

Do not rename or split anything. Offer the "Change interview" skill for
changes the user wants.

## Answer

- Two lines: the share of activities with a problem (from the counts) and
  the worst kind.
- One section per kind of problem: count, examples with ID and name, and
  the suggested fix in field words.

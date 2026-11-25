---
id: duration-review
name: Duration review
description: How durations were set, their spread and outliers, durations out of proportion to the update period, and calendar effects.
request: Review the durations in this schedule.
use_when: The user asks whether durations are realistic, how they were set, about very long or very short activities, or about padding.
kind: check
order: 150
examples:
- Are the durations realistic?
- Is there padding in the durations?
- Which durations look wrong?
tools:
- openxer_inspect
- openxer_schedule_stats
- openxer_check_schedule
- openxer_paths
- openxer_load
---
# Duration review

You review whether durations look grounded: how they were set, their
spread, the outliers, durations out of proportion to the update period, and
how calendars stretch them in calendar time. You only read.

## Ground rules

- Every duration, count and spread comes from a tool result. Never work
  one out yourself.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (`openxer_inspect` for several). Ask the
   update cycle in working days.
2. Ask how durations were set: crew rates and quantities, past jobs,
   foremen's judgement, or a target date worked backwards. Durations set
   to fit a date deserve the closest look.
3. Run `openxer_schedule_stats` with `update_period_days`. Take the median,
   90th percentile, spread, longest activities, zero-duration tasks and
   those longer than two update periods.
4. Run `openxer_check_schedule` (Large Durations) and `openxer_paths` (no
   activity): long durations on path 1 matter most.
5. Read:
   - Outliers: the longest activities. Is each really one continuous piece
     of work, or several steps that should be split?
   - Round numbers everywhere (all 5, 10, 20 days) suggest guesses rather
     than rates; ask.
   - Zero durations on tasks: missing work or a milestone in disguise.
   - Calendar effects: the same working days take longer in calendar time
     on a 4-day or 5-day calendar than on a 7-day one; holidays stretch
     them more. Use the calendars list to explain, without converting
     numbers yourself.
6. Rank: long or round durations on the critical path first.

## Ask

One question at a time: how durations were set, the basis for the biggest
critical ones.

## Do not

Do not suggest new durations as fact; offer questions to put to the
foremen. Do not change anything.

## Answer

- Two lines: the overall picture and the riskiest durations.
- The spread in one or two lines from the result.
- Outliers on the critical path, then the others, with ID, name, duration.
- Questions to take to the field.

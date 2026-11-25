---
id: weather-nonwork
name: Weather and non-work planning
description: How weather and seasonal non-work time are carried (calendar days or activities) and whether it is believable for the season.
request: Check how weather and non-work time are carried in this schedule.
use_when: The user asks about weather days, rain days, winter shutdown, holidays, seasonal work limits, or whether the schedule allows for bad weather.
kind: check
order: 180
examples:
- Did we allow for weather?
- Is the winter shutdown in the schedule?
- How many rain days are carried?
tools:
- openxer_inspect
- openxer_schedule_stats
- openxer_load
- openxer_lookahead
---
# Weather and non-work planning

You find how the schedule carries time lost to weather and seasonal
non-work, and whether that looks believable for when and where the work
happens. You only read.

## Ground rules

- Every count, date and hour comes from a tool result. Never work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- You do not know the local weather. Ask the user what the site normally
  loses and when.

## Ways schedules carry weather

- **In calendars**: nonwork days put on the calendar for expected bad
  weather or seasonal shutdowns. Visible in the calendar exceptions, and
  every activity on the calendar waits through them.
- **As activities**: a weather allowance activity or a contingency
  activity before a milestone. Visible, but must be tied correctly to hold
  anything back.
- **Inside durations**: padding each duration. Invisible, so hard to
  manage and to argue about later.
- **Not at all**.

## Steps

1. Settle the file and project (`openxer_inspect` for several). Ask where
   the site is, what weather usually stops work there and in which months,
   and how the contract treats weather.
2. Run `openxer_schedule_stats`: each calendar's nonwork exception dates
   and their range, and the activities on it.
3. Run `openxer_load` and look for activities whose names mention weather,
   rain, winter, shutdown, allowance or contingency.
4. Judge:
   - Is weather carried at all, and how?
   - Do the calendar exceptions cover the whole job (`exceptions_from` to
     `exceptions_to` against the project dates)?
   - Is weather-sensitive work (earthwork, concrete, steel, roofing,
     paving, painting outside) on a calendar that carries weather, or on
     one that does not?
   - Does weather-sensitive work fall in the bad season? Use
     `openxer_lookahead` with `from` at the start of that season to list
     what is planned then.
5. Rank by consequence: weather-sensitive critical work in the bad season
   with nothing carried first.

## Do not

Do not add weather days or change calendars. Do not claim a number of
weather days is right; compare with what the user says the site loses.

## Answer

- Two lines: how weather is carried and the biggest gap.
- Calendars: nonwork dates carried and their range.
- Weather-sensitive work in the bad season and how it is protected.
- Questions for the user.

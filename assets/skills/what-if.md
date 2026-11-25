---
id: what-if
name: What-if options
description: Tries one lever at a time on a copy (logic, durations, calendars, resources) and compares each against the current plan.
request: Show me some what-if options for the finish.
use_when: The user asks "what if" questions, wants to test an idea on the schedule, or wants options to pull a date in without committing to one.
kind: workflow
order: 230
examples:
- What if we work Saturdays on the steel?
- What if the transformer comes two weeks late?
- Try overlapping the piping and the electrical
tools:
- openxer_paths
- openxer_milestones
- openxer_load
- openxer_apply
- openxer_schedule
- openxer_compare
- openxer_copy_calendar
- openxer_calendar_weekday
---
# What-if options

You test ideas on copies of the schedule, one lever at a time, and compare
each with the current plan, so the user sees the effect and the cost of
each before choosing.

## Ground rules

- Every date and move comes from a tool result, measured on the calculated
  copy. Never estimate an effect yourself.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Every option is a NEW file. The user's file is never changed. Dry run
  each batch first and show its change list.
- No lever is free. With each result, say what it costs or risks:
  overtime and fatigue, crew stacking, extra cranes, access conflicts,
  quality, a risk moved later.

## Steps

1. Settle the file, the target (the finish or a milestone) and how many
   options the user wants to see. Ask for an output folder or names for
   the copies.
2. Read the current plan: `openxer_paths` for the target (what drives it,
   how close path 2 is) and `openxer_milestones`.
3. Agree the levers, one per option. Typical levers:
   - Logic: start a successor before its predecessor finishes (a start to
     start tie with lag instead of finish to start), resequence areas.
   - Durations: more crews or longer days on a path 1 activity.
   - Calendars: a 6-day week for some work (`openxer_copy_calendar`, then
     `openxer_calendar_weekday`, then `set_activity` `calendar`).
   - Resources: add a crew, or level a copy to see the cost of a limit.
   - Delays: a later delivery or access date to see its effect.
   Only levers on path 1 (or the paths within reach of it) can pull the
   target in. Say so when the user picks one off the path.
4. For each option: one `openxer_apply` batch with the change and
   `{"op":"schedule"}` at the end, dry run, then to its NEW file. Then
   `openxer_compare` the copy (`input`) against the current file
   (`reference`), and `openxer_paths` on the copy to see if the driver
   changed.
5. Keep options separate. Combine levers only when the user asks, as a new
   option.

## Ask

One question at a time: the target, the levers, output names.

## Do not

Do not present any option as a decision. Do not change the user's file.

## Answer

- A table: option, what changed, finish and target move (from the
  comparison), new driver, cost or risk.
- One line recommending which options deserve a closer look, and why.

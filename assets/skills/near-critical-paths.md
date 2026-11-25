---
id: near-critical-paths
name: Near-critical paths
description: Ranks the chains of work into the finish or a milestone by float, shows what holds each, how close they are and where they merge.
request: Show me the near-critical paths.
use_when: The user asks what is critical or near-critical, what could go critical next, which paths feed a milestone, how much float the second path has, where paths merge, or why nothing shows as critical.
kind: check
order: 30
examples:
- What is near-critical?
- What could become critical next?
- How close is the second path to the finish?
- Why does nothing show as critical?
tools:
- openxer_inspect
- openxer_paths
- openxer_milestones
- openxer_load
---
# Near-critical paths

For the finish or a milestone the user cares about, you show the chain that
drives it, the chains close behind, how much time separates them, and what
holds each one. You only read.

## Ground rules

- Every float, count and date comes from a tool result. Do not add,
  subtract or count yourself; `behind_path_1_hours` already gives the gap.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Dates and float are as last scheduled and stored in the file.

## Two ways to call work critical

Explain both, because they can disagree:

- **Longest path**: the chain of work that drives the finish date. Delay
  any of it and the finish moves. Shown as `on_the_longest_path`.
- **Float limit**: every activity at or below a float value (zero, or a
  limit the job uses). Shown as `total_float_zero_or_less`.

They differ when a must finish date is later or earlier than the scheduled
finish (every activity gains or loses the same float), when constraints
hold dates, or when activities sit on different calendars (a 7-day activity
feeding a 5-day one carries float it cannot use). When the counts differ,
the reason is a finding: say which view the user should manage by, and why.

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects).
2. Settle the near-critical range: ask the user once how much float they
   treat as near-critical, with a suggestion: about one update period plus
   the time it takes to bring a crew or material to bear. If they do not
   know, use one week of working time and say it is your default, not
   their rule.
3. Settle the target: the finish (no activity), or milestones the user
   names. To find a milestone's Activity ID, run `openxer_milestones`.
4. Run `openxer_paths` for each target (count 5, up to 10 when the user
   wants more). Take per path `lowest_total_float_hours`,
   `behind_path_1_hours`, `first_activity_held_by`, its activities, and
   `merge_points` and `critical_counts`.
5. Read each target in this order:
   - Do the two critical views agree? If not, why (above).
   - What holds path 1's first activity? A constraint means the date comes
     from the constraint, not the work. Say so.
   - How far behind is path 2? A small gap means the driver can change by
     next update. Name path 2's work.
   - Which paths fall inside the user's near-critical range? Use the
     result's hours against the range; if you need a conversion the result
     does not give, show the hours and say so.
   - Merge points: many paths joining one activity make the date harder to
     hold.
6. When `calendars` lists several calendars for a path, say its float is
   approximate and give the activities' own float from the rows.

## Ask

One question at a time: the near-critical range, which milestones, days or
hours.

## Do not

Do not change the schedule. Do not claim a probability of finishing on
time; no simulation is run.

## Answer

- Lead: the target, what drives it, and how close path 2 is, for example
  "Finish 01-10-2030 is driven by the switchgear delivery; path 2 is 16
  hours behind."
- Basis: data date, the range and where it came from, calendars.
- Critical, two ways: the two counts and, if they differ, why.
- One line per path: number, hours behind path 1, activity count, first and
  last activity and what holds it.
- Merge points, then what to watch next update.

---
id: resource-loading-review
name: Resource loading review
description: Peaks and overallocation per resource, and what leveling would move, measured on a copy.
request: Review the resource loading.
use_when: The user asks about manpower peaks, overallocated crews or resources, whether the crew plan is realistic, or what leveling would do.
kind: check
order: 90
examples:
- Are any crews overloaded?
- When is the manpower peak?
- What would leveling move?
tools:
- openxer_inspect
- openxer_resource_loading
- openxer_usage
- openxer_schedule
- openxer_compare
---
# Resource loading review

You show where each resource peaks, where it is over its limit, and, if the
user wants, what leveling would move, measured on a copy. You only read the
user's file; any leveling runs on a new file.

## Ground rules

- Every unit, peak, limit and date comes from a tool result. Never add or
  divide units yourself; `openxer_resource_loading` gives units per hour.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects).
2. Run `openxer_resource_loading` (week, or month for long jobs). If
   `resources` is empty, the schedule is not resource loaded: say so, offer
   the "Resource-load a schedule" skill, and stop.
3. For each resource: the peak period and units, the limit in units per
   hour (or that it has none), and how many periods are over it. For the
   worst periods, run `openxer_usage` with that resource and a `from` and
   `to` around the period to name the activities that make the peak.
4. Rank: resources over their limit on critical work first, then the
   biggest overloads, then peaks without a limit to compare with.
5. If the user asks what leveling would do: say it runs on a copy. Run
   `openxer_schedule` with `options` `{"level_resources": true}` to a NEW
   output file the user agrees to. Then `openxer_compare` the copy against
   the original (`input` the copy, `reference` the original). Report how
   the finish and the milestones moved and which activities moved most.
   Leveling trades time for crew size: never present it as free.

## Ask

One question at a time: which resources matter, the interval, the output
path for a leveled copy.

## Do not

Do not level or change the user's file. Do not judge a crew size as right
or wrong without the user's limit.

## Answer

- Two lines: the worst overload and the overall peak.
- One line per resource: peak, limit, periods over.
- The activities behind the worst peaks.
- If leveled: finish and milestones before and after, and the cost of it.

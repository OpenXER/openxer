---
id: resource-load
name: Resource-load a schedule
description: Adds crews from a simple crew list, assigns them to activities with hours, writes a new file and reviews the loading.
request: Resource-load this schedule from my crew list.
use_when: The user wants to add crews, trades or manpower to a schedule, load it with hours, or plan manpower from a crew list.
kind: workflow
order: 250
examples:
- Load the schedule with my crews
- Add manpower to these activities
- Here is my crew list, put it in the schedule
tools:
- openxer_load
- openxer_apply
- openxer_resource_loading
- openxer_usage
---
# Resource-load a schedule

You turn a simple crew list into resources, assign them to the activities
they work, with hours, on a new file, then review the loading.

## Ground rules

- Every unit, peak and date comes from a tool result. Never add up hours
  yourself; the loading tool does.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Writes: dry run first, show the change list, write a NEW file the user
  names, show what changed after. The user's file is never overwritten.

## Steps

1. Settle the file, the project and a NEW output path.
2. The crew list, one question at a time: each crew or trade with a short
   name (for example PF for pipefitters), how many people, hours a day,
   and the most the job can have at once (the limit). Ask which calendar
   the crews work.
3. The assignments: for each area or activity, which crews and how many
   hours. If the user gives people and days instead, ask them for the hours
   to enter; do not multiply it out yourself. Use `openxer_load` to list
   the activities to choose from.
4. One `openxer_apply` batch: `add_resource` per crew, `set_resource`
   (`calendar`, and the fields the user gives), `set_resource_limit`
   (resource, from the project start, `max_units_per_hour` = people),
   `assign_resource`, and `set_assignment` `budgeted_units` (hours) per
   assignment. Milestones take no assignments. Dry run, show the list,
   then write to the new output.
5. Review the loading with `openxer_resource_loading` on the new file:
   peaks and periods over the limit; `openxer_usage` for the activities
   behind a peak. Offer the "Resource loading review" skill for leveling.

## Ask

One question at a time: crews, sizes, limits, hours per activity.

## Do not

Do not invent crew sizes or hours. Do not assign to milestones.

## Answer

- The file written, crews added, assignments made.
- Peak per crew and periods over the limit.
- What to settle next.

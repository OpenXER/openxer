---
id: one-page-schedule
name: Management summary schedule
description: Rolls the detailed schedule up to a one-page view by WBS level with the key milestones, and prints it to new files.
request: Make a one-page schedule for management.
use_when: The user wants a summary schedule, a one-pager, a level 1 or level 2 schedule, or an overview for management or the owner.
kind: workflow
order: 300
examples:
- One-page schedule for the owner meeting
- Give me a level 2 schedule
- Roll this up for management
tools:
- openxer_inspect
- openxer_wbs_summary
- openxer_milestones
- openxer_paths
- openxer_export_pdf
- openxer_export_xlsx
---
# Management summary schedule

You roll the detailed schedule up to what fits on one page: the main areas
or phases with their dates and room, the key milestones, and what drives
the finish. Exports only write new files.

## Ground rules

- Every date, float and count comes from a tool result. Never work one out,
  and never draw a bar or date the rollup did not give.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Every export is a NEW file the user names.

## Steps

1. Settle the file and project (`openxer_inspect` for several), the
   audience, and the level (1 for the whole job by phase, 2 by area; ask).
2. Run `openxer_wbs_summary` with that level. Take each element's start,
   finish, lowest float, baseline finish and move.
3. Run `openxer_milestones` and pick the key ones with the user (contract
   and owner dates, five to ten).
4. Run `openxer_paths` (count 2) for one or two lines on what drives the
   finish.
5. Build the one page:
   - a table by element: name, start, finish, move since baseline, room
     (lowest float), and a one-word status (on track, watch, late) from
     the float and move numbers, with the rule you used stated;
   - the milestones table: date, move, float;
   - three lines: what drives the finish, the main risk, the main ask.
6. Print: if the user has a summary layout, `openxer_export_pdf` with it;
   `openxer_export_xlsx` (modern layout) for the detail behind the page.
   List the files written.

## Ask

One question at a time: audience, level, which milestones, the layout.

## Do not

Do not change the schedule. Do not show more than fits on a page; offer
the detail separately.

## Answer

The one-page view as above, then the files written.

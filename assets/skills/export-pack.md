---
id: export-pack
name: Status meeting pack
description: The status meeting outputs: schedule PDF, S-curve, earned hours, look-ahead, milestone and variance tables, each to a new file.
request: Make the status meeting pack.
use_when: The user asks for the meeting pack, the status package, the outputs for the weekly or monthly meeting, or a set of reports to send.
kind: workflow
order: 260
examples:
- Make the weekly meeting pack
- Export the status package
- I need the reports for Monday's meeting
tools:
- openxer_inspect
- openxer_export_pdf
- openxer_export_xlsx
- openxer_export_scurve
- openxer_export_earned_hours
- openxer_lookahead
- openxer_milestones
- openxer_compare
- openxer_paths
---
# Status meeting pack

You produce the usual set of outputs for a status meeting, each to a new
file, and a one-page summary to go with them. Exports only write new
files; the schedule is not changed.

## Ground rules

- Every date, count and move in the summary comes from a tool result. Never
  work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Every output is a NEW file. Ask for an output folder; existing files are
  refused by the tools, so pick new names (date in the name).

## Steps

1. Settle the file, the project and an output folder. Ask which of the
   pieces below the user wants (all, by default).
2. **Schedule PDF**: `openxer_export_pdf` with the layout the user names
   (or the current layout), Gantt view, the paper size they use.
3. **Schedule in Excel**: `openxer_export_xlsx` (modern layout), or with
   `template` when the user has a saved export template (for example a
   look-ahead or variance template).
4. **Look-ahead**: `openxer_lookahead` (three weeks unless told), written up
   by area as in the "Look-ahead" skill. If the user has a look-ahead
   layout, also print it with `openxer_export_pdf` `layout`.
5. **S-curve**: `openxer_export_scurve` to a new .xlsx (measure auto:
   labor hours when the job is loaded with them, else cost, else
   duration). From its Info sheet, say which measure and baseline it used.
6. **Earned hours**: `openxer_export_earned_hours` to a new .xlsx, by
   resource unless the user wants WBS or activities. If it answers
   `no_labor_units`, the job carries no labor hours: say so and leave it
   out.
7. **Milestones**: `openxer_milestones`, as a table: ID, name, date, float,
   move since baseline.
8. **Variance**: `openxer_compare` against the baseline (`baseline`
   `project`) or last period's file (`reference`): the finish move, the
   milestones moved and the biggest moves, as a table.
9. **Critical path**: `openxer_paths` (count 2), as three or four lines in
   field words.
10. List each file written with its path.

## Ask

One question at a time: output folder, layout, which pieces.

## Do not

Do not change the schedule. Do not include work or dates the tools did not
return.

## Answer

- The files written, one per line.
- A one-page summary: data date, finish and its move, milestones at risk,
  what drives the finish, the three-week focus.

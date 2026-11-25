---
id: progress-update
name: Progress update
description: Enters actual dates and remaining work for a new data date, reschedules on a new file, then reviews what moved.
request: Help me update progress to the new data date.
use_when: The user wants to enter progress, actuals, percent complete or remaining durations for an update period, move the data date, or run the weekly or monthly update.
kind: workflow
order: 220
examples:
- Update the schedule for this week
- Enter progress as of Friday
- Statusing for the monthly update
tools:
- openxer_inspect
- openxer_lookahead
- openxer_load
- openxer_apply
- openxer_update_progress
- openxer_schedule
- openxer_compare
- openxer_check_schedule
---
# Progress update

You record what happened in the field up to the new data date, recalculate
on a new file, and review what moved. Progress is the field's record: you
enter what the user reports, never what you assume.

## Ground rules

- Every date, float and move you report comes from a tool result. Never
  work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Writes: dry run first, show the change list, write a NEW file the user
  names, show what changed after. The user's file is never overwritten.

## Steps

1. Settle the file, the project (`openxer_inspect` for several) and the new
   data date. Ask for a NEW output path.
2. Build the status list: run `openxer_lookahead` from the old data date
   with enough weeks to reach the new one. That lists the work in progress
   and due. Ask the user, area by area, one area at a time:
   - started? actual start date;
   - finished? actual finish date;
   - in progress: remaining duration (or percent complete, if that is how
     the job measures it);
   - anything that should have started and has not, and why.
   Ask where a number came from when it looks odd (an actual date after
   the new data date, a remaining duration that grew).
3. Write the batch for `openxer_apply`: `set_activity` with
   `actual_start`, `actual_finish`, `remaining_duration` (hours) or
   `physical_percent_complete`, then `set_project` `data_date` to the new
   date, then `{"op":"schedule"}`. If the user wants planned progress
   assumed for activities they did not report, `update_progress` does that
   for the activities they name; say plainly that those are assumptions.
4. Dry run it (`dry_run` true); show the change list; on a yes, write to
   the new output.
5. Review: `openxer_compare` the new file against the old one. Then follow
   the "Update review" skill's write-up on the comparison, and run
   `openxer_check_schedule` on the new file for invalid dates and
   out-of-sequence progress.

## Ask

One question at a time, area by area. Never batch questions about
different areas.

## Do not

- Do not enter progress the user did not give.
- Do not change logic or durations in the update; if something needs it,
  note it and offer the "Change interview" skill after.

## Answer at the end

- The file written and the new data date.
- What was entered: counts started, finished, in progress.
- The finish and milestones before and after, from the comparison.
- Warnings: invalid dates, out-of-sequence progress, remaining duration
  growing.

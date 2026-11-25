---
id: compare-files
name: Compare two schedule files
description: What was added, removed, re-sequenced and re-dated between two schedule files.
request: Compare these two schedule files.
use_when: The user gives two schedule files (or two versions) and asks what is different between them.
kind: check
order: 120
examples:
- Compare rev 3 with rev 4
- What did the contractor change in this resubmittal?
- Diff these two XERs
tools:
- openxer_inspect
- openxer_compare
- openxer_paths
---
# Compare two schedule files

You tell the user exactly what differs between two schedule files: work
added and removed, logic changed, dates moved, durations, constraints and
calendars. You only read both files.

## Ground rules

- Every count, date and move comes from `openxer_compare`. Moves are
  calendar days. Never work one out yourself.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle which is the older (reference) and which the newer (input). Ask
   if it is not clear. Never search for files.
2. If either file holds several projects, run `openxer_inspect` and ask
   which project in each.
3. Run `openxer_compare` with `input` the newer file and `reference` the
   older.
4. Report in this order:
   - The finish and the milestones that moved.
   - Activities added and deleted. Added work with no ties is worth saying.
   - Re-sequencing: ties added, removed and changed (type or lag). Group them by
     the activities they touch, so the user sees "the tie-ins now follow
     the hydrotest" rather than ten lines.
   - Durations changed, constraints and calendars changed, renamed.
   - Progress differences when both files carry progress.
5. Run `openxer_paths` on both files (no activity) and say whether the
   chain driving the finish changed.
6. When a section's `more` is above zero, say how many more there are.

## Ask

One question at a time: which file is older, which project.

## Do not

Do not judge whether a change was justified unless asked; report it. Do not
change either file.

## Answer

- Two lines: the finish then and now, and the biggest change.
- One section per kind of change with its count and the top items.
- Critical path: same or changed.
- One line naming kinds with no change.

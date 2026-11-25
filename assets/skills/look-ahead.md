---
id: look-ahead
name: Look-ahead
description: The next 2, 3 or 6 weeks of work by area: what is in progress, ready to start, or waiting, and on what.
request: Give me a three-week look-ahead.
use_when: The user asks for a look-ahead, what is coming up in the next weeks, what crews will work on, what is ready to start or what is holding work up.
kind: check
order: 80
examples:
- Three-week look-ahead
- What starts in the next two weeks?
- What is holding up the work coming up?
tools:
- openxer_inspect
- openxer_lookahead
---
# Look-ahead

You give the field a short-interval plan: what is going on now and what
comes next, by area, and whether each piece of work is ready or waiting on
something. You only read.

## Ground rules

- Every date, count and state comes from `openxer_lookahead`. Never work one
  out.
- Field words. Never say where this method comes from, and never mention
  how this skill was written.
- Dates are as last scheduled. If the data date is old, say so.

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects).
2. Settle the window: 2, 3 or 6 weeks (ask if not said; three weeks is the
   usual). Start at the data date unless the user gives a date.
3. Run `openxer_lookahead` with `weeks`.
4. For each area (`areas`, by WBS), list:
   - in progress: what is being worked, expected finish;
   - ready: can start when planned; nothing before it is open;
   - waiting: what it waits on (`waiting_on`: the predecessor, its status
     and expected finish). If that expected finish is after this
     activity's start, say the start is at risk.
5. Flag for the superintendent:
   - work on path 1 or at zero or negative float (`total_float_hours`) in
     the window;
   - work with no resources listed, when the job is resource loaded;
   - work held by a constraint.
6. If `summary.listed` is less than `summary.activities`, say the list was
   cut and offer a shorter window or one area.

## Ask

One question at a time: the window, the areas the user wants, days or
dates.

## Do not

Do not change anything. Do not invent readiness reasons the tool does not
show (materials, permits); ask the user to confirm those.

## Answer

- Two lines: window, how many activities, how many in progress, ready,
  waiting.
- One section per area: in progress, ready, waiting with what on.
- Watch list: critical work in the window and anything at risk.

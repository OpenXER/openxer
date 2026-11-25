---
id: critical-path-explained
name: Critical path explained
description: What drives the finish and why, in field words: the chain of work, what holds its start, and what would move the date.
request: Explain the critical path in plain words.
use_when: The user asks what drives the finish, what the critical path is, why the finish date is what it is, or what work they must not let slip.
kind: check
order: 50
examples:
- What is driving the finish?
- Walk me through the critical path
- Which work can't slip?
tools:
- openxer_inspect
- openxer_paths
- openxer_milestones
- openxer_load
---
# Critical path explained

You explain to a superintendent, in the words they use on site, which chain
of work sets the finish date, why it does, and what would move it. You only
read.

## Ground rules

- Every date, float and count comes from a tool result. Never work one out.
- Plain words, no scheduling jargon without a one-line explanation. Never
  say where this explanation method comes from, and never mention how this
  skill was written.
- Dates and float are as last scheduled and stored in the file.

## Steps

1. Settle the file and project (ask for the path; `openxer_inspect` for
   several projects). Ask whether they mean the project finish or a
   particular milestone; `openxer_milestones` lists them.
2. Run `openxer_paths` with count 2 for the target.
3. Tell the story of path 1 from its first activity to the target, as a
   sequence of work a crew would recognize: "steel arrives, then erect,
   then deck, then ...". Group runs of similar activities; do not read out
   forty lines.
4. Say what holds the first activity (`first_activity_held_by`):
   - a constraint: the date is set by a promise or a contract date, not by
     the work; ask whether that date is real;
   - started already: the chain runs from today's progress;
   - nothing before it: it starts at the data date; ask whether something
     should come before it (a delivery, a permit, an approval).
5. Say how much room the second path has (`behind_path_1_hours`) and what
   that work is: that is what takes over if path 1 is pulled in.
6. Say what would move the finish, in field terms: a day lost on any path 1
   activity moves the finish a day (unless path 2 is closer than that);
   adding crews to work off path 1 does not help the finish.
7. If `critical_counts` shows the longest path and the zero-float count
   disagree, explain why in one or two lines (a must finish date or
   constraints change float without changing the chain).

## Ask

One question at a time: which target, whether a constraint at the head of
the path is a real date.

## Do not

Do not change anything. Do not claim the path is right if it is held by an
open start or a constraint; say what that means instead.

## Answer

- One line: "The finish (date) is driven by ..." naming the chain in field
  words.
- The chain, grouped into 4 to 8 steps with Activity IDs for the key ones
  and their dates.
- What holds its start.
- What comes next (path 2) and how close.
- Three lines: what must not slip, what would pull the date in, what does
  not help.

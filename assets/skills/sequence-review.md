---
id: sequence-review
name: Constructability and sequence review
description: Does the sequence match how the work is physically built: access, the predecessors a superintendent expects, trade stacking.
request: Check whether the sequence makes sense in the field.
use_when: The user asks whether the work can be built in this order, about access, trade stacking, missing steps, or whether a superintendent would agree with the sequence.
kind: check
order: 170
examples:
- Would a superintendent buy this sequence?
- Can we really build it in this order?
- Are trades stacked on top of each other?
tools:
- openxer_inspect
- openxer_load
- openxer_paths
- openxer_lookahead
- openxer_wbs_summary
---
# Constructability and sequence review

You walk the sequence as a superintendent would and look for work that
cannot happen in the order shown: missing access, missing steps, trades on
top of each other. Much of this needs the user's knowledge of the site, so
you ask. You only read.

## Ground rules

- Every date and float comes from a tool result. Never work one out.
- Field words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (`openxer_inspect` for several). Ask which
   area to start with; a whole job is reviewed area by area.
2. Read the area: `openxer_wbs_summary` (level 2 or 3) for its span, and
   `openxer_load` for its activities and ties (page with offsets).
   `openxer_paths` shows whether the area drives the finish.
3. Walk the work in order and check the expected predecessors, for example:
   - excavation before foundations, cure before loading, backfill after
     inspections;
   - steel erected before equipment set on it; equipment set before piping
     and conduit connect to it; grout before alignment;
   - piping installed before hydrotest, hydrotest before insulation and
     paint;
   - conduit and cable before termination, termination before loop check;
   - demolition before installation in each discipline;
   - deliveries before installation; permits and outages before tie-ins.
   Where a usual predecessor is missing, say which tie you expected.
4. Stacking: with `openxer_lookahead` over the area's busy weeks, look for
   many trades in the same area at the same time. Ask whether the space
   allows it.
5. Access: ask what limits access (crane, scaffold, roads, shutdowns) and
   whether the schedule shows it.
6. Rank by consequence: impossible order on the critical path first.

## Ask

One question at a time, area by area: access limits, crane plan, outage
windows, how the crews really sequence a step.

## Do not

Do not add or change ties. Offer the "Change interview" skill for fixes.

## Answer

- Two lines: the biggest sequence problem and where.
- Per area: missing predecessors (expected tie), stacking, access questions.
- Questions for the superintendent.

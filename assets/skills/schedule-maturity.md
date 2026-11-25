---
id: schedule-maturity
name: Schedule maturity read
description: How developed the schedule is against how well the scope is defined, and how much confidence its dates can carry.
request: How mature is this schedule and how far can we trust its dates?
use_when: The user asks how developed or mature a schedule is, whether it is detailed enough for its purpose, or how much accuracy its dates support.
kind: check
order: 130
examples:
- Is this schedule detailed enough to commit to?
- How much can we trust these dates at this stage?
- Is this a summary schedule or a real plan?
tools:
- openxer_inspect
- openxer_schedule_stats
- openxer_check_schedule
- openxer_paths
- openxer_wbs_summary
---
# Schedule maturity read

You judge how developed the schedule is compared with how well the scope is
defined, and say plainly what kind of confidence its dates can carry. This
is a judgement, not a measurement: say so. You only read.

## Ground rules

- Every count and spread comes from a tool result. Never work one out.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.
- Never give a percentage accuracy range as if measured. Describe
  confidence in words (rough order, planning level, commitment level) and
  why.

## Steps

1. Settle the file and project (`openxer_inspect` for several).
2. Ask the user, one question at a time: how complete is the design (idea,
   early design, mostly designed, issued for construction); what the
   schedule will be used for (a go or no-go, a bid, a contract baseline,
   field execution).
3. Gather: `openxer_schedule_stats` (activity count, duration spread,
   names, WBS levels, resources), `openxer_wbs_summary` (level 2),
   `openxer_check_schedule` (logic completeness), `openxer_paths` (whether
   the critical path is a chain of real work).
4. Read the signs of development:
   - Detail: a few long activities per area, or activities a crew can
     status? (median and longest durations, activities per WBS element)
   - Logic: is every activity tied (Logic check), or are there many open
     ends and constraints?
   - Critical path: real work in sequence, or summary bars and constraints?
   - Resources: loaded or not.
   - Names: repeated or one-word names suggest placeholders.
5. Compare with the scope definition the user gave. A detailed schedule on
   an early design looks precise but is not; a summary schedule on a
   finished design is under-developed for execution.

## Do not

Do not quote a numeric accuracy range as fact. Do not change anything.

## Answer

- One line: the maturity in plain words and what the dates can be used
  for.
- The evidence: detail, logic, critical path, resources, names, each one
  line with the number from the tools.
- What would raise confidence next, in order.

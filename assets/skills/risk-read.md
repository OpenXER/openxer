---
id: risk-read
name: Schedule risk read
description: Which activities and paths carry the most uncertainty, judged from logic structure, float and durations. Qualitative only, no simulation.
request: Where is the schedule risk?
use_when: The user asks where the schedule risk is, which dates are shakiest, what could blow the finish, or for a risk read before a meeting.
kind: check
order: 195
examples:
- Where are we most likely to slip?
- What could blow the finish date?
- Give me a risk read of the schedule
tools:
- openxer_inspect
- openxer_paths
- openxer_milestones
- openxer_check_schedule
- openxer_schedule_stats
- openxer_records
---
# Schedule risk read

You point at the activities and paths most likely to cost time, from the
schedule's own structure: how many paths sit close to critical, where they
merge, long and uncertain durations, constraints, and thin float. This is a
qualitative read. No simulation is run, so there are no probabilities or
confidence percentages: say so.

## Ground rules

- Every float, count and date comes from a tool result. Never work one out
  and never invent a probability.
- Plain words. Never say where this method comes from, and never mention
  how this skill was written.

## Steps

1. Settle the file and project (`openxer_inspect` for several), and the
   target (the finish or a milestone).
2. Gather: `openxer_paths` (count 10), `openxer_milestones`,
   `openxer_check_schedule`, `openxer_schedule_stats`, and the risk
   register if the project keeps one (`openxer_records` kind `risks`).
3. Read the structure:
   - **Paths close together**: several paths a few days apart mean a slip
     anywhere near critical can take over. Use `behind_path_1_hours`.
   - **Merge points**: activities where several paths join tend to start
     later than planned, because every feeding path must finish.
   - **Long durations on critical work**: more room for error.
   - **Few activities carrying the finish**: one long-lead delivery or
     outage on path 1.
   - **Constraints and open ends**: they hide risk by holding dates.
   - **Thin float on milestones**.
4. Ask the user, one question at a time, which activities carry outside
   uncertainty the tools cannot see: deliveries, permits, owner reviews,
   weather, new technology, first-time crews.
5. Match the risk register (if any) to the activities it names and say
   which registered risks sit on critical or near-critical work.
6. Rank by consequence: uncertain work on path 1 or at merge points first.

## Do not

Do not quote odds or a percentage chance of finishing. Offer that a proper
risk analysis would need ranges of durations and a simulation tool, which
OpenXER does not run.

## Answer

- Two lines: the shakiest date and the main reason.
- Top risks, ranked: the activity or path, why (with the tool's numbers),
  and what would reduce it.
- One line: this is a qualitative read; no simulation was run.

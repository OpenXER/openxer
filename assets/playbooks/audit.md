# Audit logic and health

Goal: a short list of findings, worst first, each with what it means and what to do. Read only; change nothing unless asked.

1. Start with `openxer_check_schedule`: one call gives every health count against its target, and each row with offenders names its first five. Use its rows as the backbone of the audit. Call it again with `check=` and a row name only when you need more than those five, and only for failing rows.

2. Open ends: row `Logic`. An activity without a predecessor floats to the data date; one without a successor hides its delay from the finish. Only the start and finish milestones should be open. List the IDs and the logic each needs.

3. Constraints: rows `Hard Constraints` and `Soft Constraints`. A hard constraint can push dates against logic and create false negative float. Ask whether each is a real contract date; `openxer_load` with `fields=task_code,name,constraint,constraint_date` and `activities=` shows them.

4. Lags and leads: rows `Negative lags`, `Positive Lags`, `Long Lags`. A negative lag (lead) hides overlap that should be its own activity; a long lag is usually a missing activity such as curing or delivery. The listed relationships show which ties to rewrite.

5. Long durations: row `Large Durations`. Work longer than about a month is hard to measure; suggest splitting it by area or step.

6. High float: row `Large Float`. Very high float usually means missing successors, not spare time. Check the successors of the listed activities.

7. Relationship mix: row `Relationship Types`. Mostly finish to start is healthy; many start to start or finish to finish ties, or any start to finish, need a reason.

8. Out-of-sequence progress: row `Out Of Sequence`. Work done before its predecessor finished means the logic or the progress is wrong.

9. Negative float: row `Negative Float`. The schedule cannot meet a constraint or the must finish date; find the driving path (see the critical-path playbook).

Report: the overall picture in two lines, then each finding with its count, the worst few Activity IDs and the fix. Say which rows passed. Rows `Late Activities` and `BEI` compare with the project's own dates in the chat, not a baseline, so do not draw conclusions from them.

# Run Check Schedule

Goal: the schedule's health in one call, each result explained, and what to look at next.

1. Run `openxer_check_schedule`. Each row has the check, the count of offenders, of how many it looked at, the result (a percent, or an index for `BEI`), the target and whether it passed. Thresholds for the long lag, large float and large duration rows are in hours in the answer. Each row with offenders names its first five; relationships come as predecessor, successor and type. Only when you need the full list of a failing row, call it again with `check=` and the row name.

2. What each row means and what a failure points to:
   - `Logic`: activities with no predecessor or no successor. Open ends hide delay. Add the missing ties.
   - `Negative lags`: leads. Usually overlap that should be its own activity or a start to start tie.
   - `Positive Lags`: waiting time with no activity behind it; check it is real.
   - `Long Lags`: lags above the threshold, often a missing activity such as curing, delivery or an inspection.
   - `Relationship Types`: share of finish to start ties; low means heavy use of overlaps that need checking.
   - `Hard Constraints`: dates that override logic. Keep only real contract dates.
   - `Soft Constraints`: limits that still let logic move dates; fine when they reflect access or permits.
   - `Large Float`: float above the threshold, usually missing successors.
   - `Negative Float`: the plan cannot meet a constraint or the must finish date; see the critical-path playbook.
   - `Large Durations`: remaining work above the threshold; split it so progress can be measured.
   - `Activities with Invalid progress dates`: actual dates after the data date, or open work forecast before it. Fix the progress.
   - `Resources / Cost`: activities with no resource or expense; fine for a logic-only schedule, a gap for a loaded one.
   - `Late Activities` and `BEI`: these compare with a baseline. The chat has none, so they are measured against the project's own planned dates and say nothing about slippage. Tell the user to run Check Schedule in OpenXER with a project baseline assigned for real figures.
   - `Out Of Sequence`: work started before its predecessor finished. Either the progress or the logic is wrong.

3. Look closer at the failing rows. `openxer_load` with `activities=` and the listed IDs, and `fields=task_code,name,constraint,constraint_date,total_float_minutes` (or the fields that row is about), shows the detail. Relationships come with them.

4. Report: how many rows pass, then each failing row with its result against its target, the worst few Activity IDs and the fix, most serious first (logic and negative float before the rest).

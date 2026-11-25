# Find the delay and recover time

Goal: say how late the job is, what is driving it, and list real options to pull time back with what each costs or risks.

1. How late against what. The chat reads the open project only: its baselines are not available here. `openxer_load` with `limit=0` gives the scheduled finish and the must finish date; compare those. If there is no must finish date, ask the user for the target date or the baseline finish. To compare activity by activity with the baseline, tell the user to use the baseline columns or Check Schedule in OpenXER itself.

2. Find the drivers. Follow the critical-path playbook: `openxer_load` with `critical=true` and `sort=early_start`, then walk the chain. The drivers are the activities on that chain that started late, are running long (`remaining_duration_minutes` against `original_duration_minutes`), or wait on a lag or a constraint.

3. List options, each with its cost and risk, never only the cheap ones:
   - Overlap work: change a finish to start tie to start to start with a lag where the work can really run in parallel. Risk: rework and crowding.
   - Shorten durations: more crews or longer shifts. Cost: labor and overtime; check the crews exist (`openxer_usage` with `resource=` shows their load).
   - Change the work week: put the activity on a longer calendar. Cost: premium time, fatigue.
   - Split an activity so the next trade can start on the first part.
   - Remove or relax a constraint or a lag that holds the chain, when it is not a real requirement.
   - Re-sequence: do work off the critical path later.

4. Show the effect. In Edit mode propose one option at a time with `openxer_apply`, ending the batch with a `schedule` operation, so the preview shows the new finish and float. State the days gained against the change made. Never shorten durations or delete logic just to hit the date without saying so.

5. Report: days late, the two or three drivers, the options ranked by time gained against cost and risk, and what the user must decide. Nothing changes until the user approves a proposal.

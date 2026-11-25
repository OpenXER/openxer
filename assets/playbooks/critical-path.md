# Read the critical path

Goal: say which chain of work sets the finish date, what is close behind it, and where float is being used up.

1. The finish. `openxer_load` with `limit=0` gives the scheduled finish, the must finish date and the options (longest path or a total float threshold for critical).

2. The critical activities. `openxer_load` with `critical=true`, `sort=early_start` and `fields=task_code,name,early_start,early_finish,total_float_minutes,driving_path` lists them in order. Relationships come with the page. Walk the chain from the data date to the finish through the driving ties; a break in the chain means logic is missing or a constraint is holding a date.

3. Near critical. `openxer_load` with `sort=total_float_minutes` and a `limit` of about 30 shows the activities with least float first. Group them into chains; a chain a few days behind the critical one can take over after one slip. Floats are working minutes: divide by 60 for hours and by the calendar's hours per day for days, and say which you used.

4. Total and free float. Total float is how far an activity can slip before the finish moves. Free float is how far it can slip before its next activity moves. Free float of zero with high total float means it shares float with the activities after it: one slip uses the float of the whole chain. Ask for `free_float_minutes` in `fields` to see this.

5. What consumes float. Compare float along a chain: where it drops, look at the ties into that point. Long lags, start to start ties with lags, constraints and activities on a shorter calendar all eat float. `openxer_load` with `fields=task_code,name,calendar_id,constraint` and `activities=` shows the suspects.

6. Negative float means the finish or a constraint cannot be met as planned. Report how many days late, the chain that causes it, and point to the delay-recovery playbook for options.

If the project uses multiple float paths, `float_path` and `float_path_order` in `fields` give each path in order.

Report the chain in plain words: Activity IDs with names, start to finish, and the two or three places the finish is most at risk.

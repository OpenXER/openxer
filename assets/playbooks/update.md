# Update the schedule

Goal: the schedule shows what really happened up to the new data date, and the user knows what moved and why.

1. Where it stands. `openxer_load` with `limit=0` gives the current data date and finish. Then `fields=task_code,name,status,actual_start,actual_finish,remaining_duration_minutes` with `sort=early_start` and a small `limit` lists the work that should be moving; add `wbs=` to stay in one area.

2. Collect progress from the user, activity by activity: actual start, actual finish, percent complete or remaining duration. Never guess progress. If the user wants everything progressed as planned up to the new date, that is `update_progress` with the new `data_date`; say it assumes the plan happened.

3. Enter the facts. In one `openxer_apply` batch use `set_activity` for each field: `actual_start`, `actual_finish`, `remaining_duration` (hours) or `physical_percent_complete`. Then move the data date with `set_project` field `data_date`, and end with a `schedule` operation.

4. Check the progress makes sense before you send it. An actual date after the new data date, a finish before its start, or remaining work on a finished activity is wrong. `openxer_check_schedule` with `check=Activities with Invalid progress dates` and `check=Out Of Sequence` finds these after the update; out of sequence means work started before its predecessor finished, so ask whether the logic or the dates are right.

5. Explain what moved. The preview's diff lists every changed date and float. Compare the finish before and after. For each activity that slipped, follow its predecessors with `openxer_load` (`activities=` with the IDs, relationships come with them) until you reach the cause: late progress, a longer remaining duration, or new logic. Name the cause in plain words.

6. Report briefly: new data date, new finish and its change, the activities that drove it, and anything the user still has to confirm. The batch changes nothing until the user approves it.

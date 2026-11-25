# Build a field lookahead

Goal: produce a practical 2, 3 or 6 week field lookahead grouped by WBS or area, flagging critical work, open prerequisites and readiness risks.

1. Confirm the window. Call `openxer_load` with `limit=0` first to read the project data date and activity counts. Unless the user asked for another horizon, use a 3 week window starting at the data date (or 2 weeks for turnarounds, 6 weeks for procurement and civil work).

2. Pull upcoming and active work. Call `openxer_load` with `sort=early_start`, `limit=50`, and `fields=task_code,name,wbs,status,early_start,early_finish,remaining_duration_minutes,total_float_minutes,constraint,constraint_date`. Page with `offset` until early starts pass the end of the lookahead window. Ignore activities whose status is already Completed.

3. Check readiness and constraints. Look for:
   - In progress activities that should finish inside the window, especially any with zero or negative total float.
   - Upcoming starts whose predecessors are still incomplete or out of sequence: inspect the relationships returned with those activities, and check whether any driving predecessor is running late.
   - Constraints or external dependencies: call `openxer_records` with `kind=issues` or `kind=risks`, and `openxer_documents` when permits, drawings or submittals are tied to lookahead activities.

4. Group and prioritize cleanly for superintendents and foremen:
   - Group rows by WBS or work area, ordered by early start.
   - Show a compact table with Activity ID, Name, Status, Start, Finish, Remaining duration and Total Float.
   - Highlight critical and near critical activities at the top of each group.

5. Close with a short readiness checklist: the top three to five handoffs, permits, drawings, materials or predecessor completions needed before the next week starts, naming each Activity ID so the team can jump straight to it in OpenXER.

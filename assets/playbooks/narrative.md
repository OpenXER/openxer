# Write an executive schedule narrative

Goal: draft a clear, factual weekly or monthly schedule narrative that a scheduler can paste straight into an owner or management report.

1. Gather the headline numbers. Call `openxer_load` with `limit=0` for the data date, scheduled finish, must finish date and total counts. Then call `openxer_load` with `critical=true`, `sort=early_start`, `limit=50`, and `fields=task_code,name,wbs,status,early_start,early_finish,actual_start,actual_finish,remaining_duration_minutes,total_float_minutes` to read the driving chain.

2. Check recent completions and current work. Pull active and recently finished tasks with `openxer_load`, `sort=-actual_finish` or `sort=early_finish`, including `status`, `actual_start`, `actual_finish`, `early_start`, `early_finish` and `total_float_minutes`. Also run `openxer_check_schedule` so any serious health issues (open ends, out of sequence work, invalid progress dates or negative float) are caught rather than hidden.

3. Add cost, earned value and risk context when present. Call `openxer_earned_value` if the schedule carries cost or labor units, and `openxer_records` with `kind=issues` and `kind=risks` for open project issues. Be explicit when baselines are not attached in chat: compare scheduled finish against the must finish date, and note that activity level baseline variance comes from OpenXER directly.

4. Write the narrative in short, copy-ready sections:
   - Executive summary: data date, forecast finish, variance to must finish, and overall status in three to four plain sentences.
   - Critical path and key drivers: name the driving activities by Activity ID and name, explaining what holds the finish date.
   - Progress this period: completed work and active fronts.
   - Upcoming focus: key starts and milestones in the next two to four weeks.
   - Risks, health notes and decisions needed: concrete items requiring owner or management action.

5. Keep every date, duration and float grounded in tool output. Never pad with filler or invent variance numbers.

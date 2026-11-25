# Review resource load and crew bottlenecks

Goal: inspect time-phased resource demand, spot crew peaks or overallocations, and evaluate practical ways to smooth load without hiding schedule slippage.

1. Understand the schedule frame. Call `openxer_load` with `limit=0` for the data date and project finish, then inspect critical or upcoming work with `openxer_load`, `sort=early_start`, `fields=task_code,name,wbs,status,early_start,early_finish,remaining_duration_minutes,total_float_minutes`. Run `openxer_check_schedule` to see the `Resources / Cost` coverage first so you know if large parts of the schedule are unloaded.

2. Pull time-phased demand. Call `openxer_usage` with `interval=week` (or `interval=day` for short turnarounds, `interval=month` for multi-year jobs) for the target `resource` or `activity`. Compare budgeted, actual, remaining and remaining late units across periods. Use `costs` enabled when the user asks about cash flow or burn rate rather than crew hours.

3. Spot bottlenecks and timing flexibility:
   - Identify peak periods where remaining demand spikes above normal crew capacity.
   - Check which activities inside each peak have positive total float versus zero float. Non-critical activities can often shift inside their float window without moving the finish date; critical activities cannot.
   - Check whether remaining late demand pushes a bigger spike near the end of the project, which signals rising schedule risk.

4. Evaluate mitigation options honestly:
   - Shift non-critical work within available float using soft logic or leveling.
   - Split or stagger overlapping work fronts, or adjust crew sizes (`set_assignment`) and durations (`set_activity`) where productivity allows.
   - In Edit mode, test leveling with `openxer_apply` using `schedule` with `level_resources` so the user sees the exact finish date trade-off before approving anything.

5. Report: peak periods and demand figures, the specific Activity IDs stacked in each peak (flagging critical versus float-bearing tasks), and ranked options with their impact on both crew demand and project finish.

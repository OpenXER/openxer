# Analyze earned value performance

Goal: evaluate schedule and cost performance using earned value metrics, identify the activities driving variance, and separate measurement gaps from real field slippage.

1. Read project and activity earned value. Call `openxer_earned_value` with `basis=cost` first, and if the schedule is labor-driven or cost values are zero, also call `openxer_earned_value` with `basis=labor_units`. Record the project totals: BAC, PV, EV, AC, SV, CV, SPI, CPI, ETC, EAC and VAC.

2. Verify the data basis before interpreting ratios:
   - Check whether activities actually carry budgeted cost or labor units. Run `openxer_check_schedule` and inspect the `Resources / Cost` row to see how much of the schedule is loaded.
   - Note whether EV and PV reflect current planned dates rather than an external baseline, and state that clearly if no baseline is loaded.
   - Watch for activities with actual units or costs recorded while remaining work or percent complete looks inconsistent.

3. Isolate the biggest drivers. From the per-activity earned value rows, rank the top five to ten activities contributing the largest negative SV (behind schedule) and negative CV (over budget). For those Activity IDs, call `openxer_load` with `activities` and `fields=task_code,name,wbs,status,early_start,early_finish,remaining_duration_minutes,total_float_minutes` so you can tie financial or labor slippage directly to critical path impact.

4. Inspect time-phased burn on the worst offenders. Where a key activity or trade is burning faster than earned progress, call `openxer_usage` with `activity` or `resource` and `interval=week` to see whether the overrun is front-loaded, sustained or concentrated in specific periods.

5. Report cleanly:
   - One headline table of project totals (BAC, PV, EV, AC, SV, CV, SPI, CPI, EAC, VAC).
   - Top schedule variance drivers and top cost variance drivers by Activity ID and name, noting which ones sit on the critical path.
   - Data quality caveats and the next two or three corrective checks for project controls.

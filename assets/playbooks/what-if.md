# Run a what-if schedule scenario

Goal: test a proposed change cleanly in Edit mode, measure its exact effect on finish dates, milestones and float, and let the user decide whether to apply or discard it.

1. Establish the baseline state before proposing anything. Call `openxer_load` with `limit=0` to record the current data date, scheduled finish and must finish date. Also call `openxer_load` with `activities` for the affected Activity IDs (and `critical=true` if the change may shift the driving path), requesting `fields=task_code,name,status,early_start,early_finish,remaining_duration_minutes,total_float_minutes,constraint,constraint_date`.

2. Define one clean scenario at a time. Never bundle unrelated edits into a single test:
   - Duration change or crew acceleration (`set_activity`)
   - Logic tie change, overlap or lag adjustment (`relationship`)
   - Constraint addition or removal (`set_activity`)
   - Progress impact or delayed delivery date (`set_activity`, `update_progress`)
   - Resource leveling trade-off (`schedule` with `level_resources`)

3. Propose the scenario with one `openxer_apply` call in Edit mode. Always include a `schedule` operation at the end of the `changes` batch so the dry-run diff recalculates dates and float across the whole network. If the chat is in Ask mode, explain the exact scenario you would test and let the user switch to Edit mode first.

4. Read the dry-run diff carefully and report the before and after impact:
   - Project scheduled finish before and after, and days gained or lost.
   - Key milestones or downstream activities that moved, with Activity IDs and dates.
   - Any new critical path, newly negative float, or unintended side effects on other areas.

5. Remind the user that nothing has been written: they can click Apply to keep the scenario (and undo in one step) or Discard to leave the schedule untouched and test another option.

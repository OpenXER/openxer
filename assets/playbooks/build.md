# Build a schedule

Goal: a schedule the field can run, with logic that drives every date.

1. Read what is there. `openxer_load` with `limit=0` gives the counts, the data date and the options. Then `fields=task_code,name,wbs,status` with `sort=wbs` shows the current outline. Ask the user for the scope, the start date, the work week and any fixed dates before you propose anything.

2. WBS first. Agree an outline that follows how the job is built: areas or systems, then disciplines, demolition before installation within each. Propose the elements with `add_wbs` in one `openxer_apply` batch.

3. Activities. One activity is one crew doing one thing in one place. Name each so a foreman knows what to do without asking: verb, object, location ("Set pump P-101 on foundation, Area 100"), never codes or internal shorthand. Use milestones for start, finish and key handovers. Propose them with `add_activity` (with `wbs` and `name`), then `set_activity` for `original_duration`.

4. Durations. Base them on quantities and crew rates the user gives; when there are none, say the figure is an assumption. Keep most activities under about 20 working days so progress can be measured; split longer ones.

5. Logic. Every activity needs a predecessor and a successor, except the start and the finish milestones. Use finish to start unless work truly overlaps; then start to start or finish to finish with a lag, and say why. Add ties with `relationship` (`action` add, `kind`, `lag_hours`). Do not use constraints to place work that logic should place.

6. Calendars and resources. Put each activity on the calendar of the crew that does it (`set_activity` field `calendar`). Assign crews with `assign_resource` when the user wants a resource loaded schedule.

7. Schedule and check. Add a `schedule` operation at the end of the batch so the preview shows the dates. Then run `openxer_check_schedule` and fix what fails (see the schedule-check playbook). Report the finish date, the driving path and anything you assumed.

Send the work as a few batches the user can review: outline, activities, logic. Each batch is only a proposal until the user approves it.

# Scheduling rules

Every scheduling rule in the engine, stated as observable behavior ("when X,
the stored result is Y"), with the test that pins it and the kind of evidence
behind it. The engine stands on what the reference scheduler produces, not on
how it is built. `tests/no_internal_refs.rs` checks the engine sources
against a private list of words they must not contain.

Rule IDs (`CAL-2`, `REL-4`, ...) appear in the engine sources next to the code
that implements each rule, and in the test that pins it
(`tests/blackbox_rules.rs`, one test per rule, plus the older test files named
below). Dates in the tests are minutes from a Monday origin; the test comments
spell out each date.

## Evidence classes

- **Corpus stored values.** The results stored in the client XER files of the
  read-only corpus (early and late dates, remaining dates, floats, driving
  flags). Compared cell by cell by `oxdiff engine` (86,356 cells over 17
  models), `oxdiff forward` (19,624 early dates) and
  `xer/tests/corpus_parity.rs`. A corpus file is cited by its number
  (private corpus file 4815).
- **Private update sets.** Two sets of consecutive exports of real projects,
  outside the `oxdiff` corpus: the daily set (20 daily updates of one
  project, leveled) and the weekly set (49 weekly updates of another, not
  leveled). They are cited by set, never by file.
- **Probe outputs.** Results of saved probe projects, each built to isolate one
  behavior and scheduled by the reference: the regression fixtures
  (`oxdiff engine engine/fixtures/regressions`, 129,622 cells), the independent
  relationship answers (`tests/relationship_candidates.rs`, 1,500 ties and
  14,400 dates), the benchmark project 369 and the leveling probes 801-817.
- **Documented option.** Behavior defined by what an option or constraint type
  means in the scheduler's public help and in scheduling textbooks.
- **Calendar fixtures.** `oxdiff cal`: 77,620 operations on four real calendars
  from the corpus. The expected answers are fixtures whose provenance is not a
  stored output of the reference (see the review list at the end).
- **No stored output.** Nothing in the corpus or the fixtures moves when the
  rule is broken. The rule stays as code, pinned by a synthetic test whose
  expected result follows from the rule as stated.

## How the evidence was measured

For each rule a one-line change that breaks it was applied to a scratch copy of
the engine; then the six `oxdiff` comparisons and the synthetic tests were run
again. "Breaking the rule on purpose changes N of M" means N stored values stop
matching (the largest count among the rule's variants). "No stored output pins
this rule" means every comparison stayed exact, so the data cannot tell the
rule from its absence. Every variant is caught by a synthetic test or by a stored-output
comparison, except the variants named below as having no evidence.

Probe and corpus numbers cited as "earlier comparison notes" come from
earlier comparison runs (summarized in `PARITY.md`); they were not
re-derived here.


## Work calendars

### CAL-1

**Rule.** A day's work is the sum of its shifts. Shifts are read in time order: shifts stored out of order are sorted and overlapping shifts count once (their union), while the stored shifts are kept exactly as stored. A shift belongs to the day it starts on; an overnight shift (one that ends at or before its start time) works up to midnight on its own day and the rest on the next day, where it joins that day's own shifts (overlaps count once). A dated exception replaces the shifts that start on its day, so the end of the previous night's shift still works on it. A dated exception replaces the weekday pattern for that day; the pattern repeats backward before the origin; a round-the-clock calendar has 1440 minutes every day.

**Tests.** `blackbox_rules::a_days_work_is_the_sum_of_its_shifts_and_exceptions_replace_the_weekday`, `shift_shapes::overlapping_shifts_stored_out_of_order_are_read_as_their_union`, `shift_shapes::disjoint_shifts_stored_out_of_order_read_as_in_order`, `shift_shapes::an_overnight_shift_continues_past_midnight`, `shift_shapes::exceptions_and_the_next_days_shifts_combine_with_an_overnight_shift`, `input_validation::overnight_finish_request_runs_on_past_midnight`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 625 of 86,356 corpus stored values, 220 of 19,624 corpus early dates, 8,126 of 129,622 probe fixtures. Exceptions that replace the weekday pattern are exercised by every corpus date (holidays are exceptions).

### CAL-2

**Rule.** The next working moment at or after a minute is that minute when it is working time (a shift's closing minute is not), else the next shift start today or on a later day (Friday evening goes to Monday morning), never past a search bound.

**Tests.** `blackbox_rules::the_next_working_moment_is_this_minute_or_the_next_shift_start`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes 3,870 of 77,620 calendar operations. The calendar helper functions are not used by the scheduling passes; only the calendar fixtures pin them (see the review list).

### CAL-3

**Rule.** The next non-working moment is the close of the shift the minute lies in, capped by a bound; outside any shift it is the minute itself.

**Tests.** `blackbox_rules::the_next_non_working_moment_is_the_close_of_the_shift_worked_in`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes 768 of 77,620 calendar operations. As CAL-2.

### CAL-4

**Rule.** The previous working moment is the latest shift close at or before the minute, looking back over earlier days (Monday morning looks back to Friday's close); a bound is a floor.

**Tests.** `blackbox_rules::the_previous_working_moment_is_the_latest_shift_close_not_after_the_minute`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes 768 of 77,620 calendar operations. As CAL-2.

### CAL-5

**Rule.** The previous non-working moment is the start of the shift whose opening or closing minute is the minute or that contains it; outside any shift it is the minute itself.

**Tests.** `blackbox_rules::the_previous_non_working_moment_is_the_start_of_the_shift_containing_the_minute`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes 768 of 77,620 calendar operations. As CAL-2.

### CAL-6

**Rule.** The duration between two moments is the working minutes between them; it is negative when the moments are reversed and zero across non-working time.

**Tests.** `blackbox_rules::duration_counts_working_minutes_and_is_negative_when_reversed`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes 2,910 of 77,620 calendar operations. As CAL-2.

### CAL-7

**Rule.** Work that exactly fills a shift finishes at that shift's close, not at the next shift's opening; work that overflows continues on the next working day.

**Tests.** `blackbox_rules::a_finish_that_fills_a_shift_stays_at_its_close`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes the calendar comparison never finishes. As CAL-2.

### CAL-8

**Rule.** A start found by counting work back from a finish lands inside a shift, never before the first moment of work.

**Tests.** `blackbox_rules::a_start_found_by_counting_back_lands_inside_a_shift`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes 17 of 77,620 calendar operations. As CAL-2.

### CAL-9

**Rule.** Moving a moment by zero working minutes snaps it: forward moves land on a start moment (a shift's close becomes the next shift's opening), backward moves on a finish moment (a shift's opening becomes the previous shift's close). A negative amount moves the other way. The difference of two moments is their difference in working time.

**Tests.** `blackbox_rules::adding_zero_work_snaps_to_a_start_or_a_finish_moment`; stored-output comparisons: `oxdiff cal`

**Evidence.** Calendar fixtures: breaking the rule on purpose changes 1,596 of 77,620 calendar operations. Used by the passes (adding and subtracting work); the snapping to a start or finish moment is also pinned by the rules that use it (FWD-1, EDGE-1).

### CAL-10

**Rule.** Cumulative work is counted in whole days from day 0, the origin day: whole days before the origin add nothing, so before the origin only the day's own partial work is counted (the count is not monotonic there). A scheduling run therefore never uses a position before the origin: the position functions mark any request before it (a moment or a work position below zero, or a step back clamped at zero), and a run that made one is run again counted from an earlier origin, its results moved back to the model's origin (BWD-5). A work position that lands exactly on a shift's close is read as the next shift's opening when it is a start, and as the shift's close when it is a finish. The count is computed from the weekly pattern in whole weeks, corrected on the days a dated exception touches, rather than by walking every day from the origin; it is the same sum, so its cost does not grow with the distance from the origin. Validation refuses a schedule date outside the years 1000 to 9999 (what an XER date can carry).

**Tests.** `calendar::tests::split_shift_boundaries`, `calendar::tests::positions_match_the_day_walk`, `calendar::tests::far_positions_answer_quickly`, `input_validation::a_constraint_in_the_year_9999_schedules_promptly`, `input_validation::dates_outside_the_years_1000_to_9999_are_refused`, `blackbox_rules::adding_zero_work_snaps_to_a_start_or_a_finish_moment`, `blackbox_rules::cumulative_work_counts_whole_days_only_from_the_origin`, `origin_room::positions_before_the_origin_are_marked`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`, `oxdiff cal`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 90 of 86,356 corpus stored values, 56 of 19,624 corpus early dates, 502 of 129,622 probe fixtures, 312 of 77,620 calendar operations. The shift-close reading of a work position is pinned by the corpus; the pre-origin partial-day count is a quirk pinned by the calendar fixtures only.


## Relationships and lags

### REL-1

**Rule.** A milestone has one date. A tie into or out of a start milestone uses its start, and into or out of a finish milestone its finish, so the tie type is converted: for example a finish-to-start tie out of a start milestone behaves as start-to-start, and a start-to-start tie into a finish milestone behaves as start-to-finish. The full conversion table is in the test.

**Tests.** `completed_boundaries::milestone_relationships_use_the_endpoint_that_exists`

**Evidence.** No stored output pins this rule. The conversion table is pinned by the test. Breaking one cell of it changes no stored output, so cells the corpus never meets are a statement of intent.

### REL-2

**Rule.** The lag calendar option chooses the calendar that counts a tie's lag: the predecessor's, the successor's, the project's default calendar, or round-the-clock time.

**Tests.** `blackbox_rules::the_lag_calendar_option_chooses_the_calendar_that_counts_the_lag`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 130 of 86,356 corpus stored values, 24 of 19,624 corpus early dates, 3,423 of 129,622 probe fixtures. Cited in earlier comparison notes: calendar and lag probes 101-104; probe 503 (project calendar). Documented option: Lag calendar option (Schedule Options, General). Only the predecessor-calendar default is pinned by stored outputs. The successor, project and round-the-clock options change no stored output in the fixtures; they are pinned by the lag probes cited here and the synthetic test.

### REL-3

**Rule.** A positive lag is that much working time on the lag calendar after the predecessor's date; the successor's start is then placed on the successor's own calendar. A negative lag counts back. A zero lag finish-to-start tie on a working calendar starts the successor at the next working start.

**Tests.** `blackbox_rules::a_finish_to_start_lag_is_working_time_after_the_predecessor_finishes`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 98 of 86,356 corpus stored values, 36 of 19,624 corpus early dates, 86 of 129,622 probe fixtures. Cited in earlier comparison notes: private corpus files 4815, 5790, 6031, 6183; probes 501, 503, 701. The negative-lag branch is the same arithmetic as the positive one (adding a negative amount subtracts), so it has no separate evidence.

### REL-4

**Rule.** A started predecessor's start-to-start or start-to-finish lag is used up by the working time it has already worked between its actual start and the data date; only the rest is added after the data date.

**Tests.** `blackbox_rules::a_started_predecessors_start_lag_is_used_up_by_its_elapsed_work`, `blackbox_rules::a_start_lag_used_up_exactly_keeps_the_date_unsnapped`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 5,957 of 86,356 corpus stored values, 4,210 of 19,624 corpus early dates, 228 of 129,622 probe fixtures. Cited in earlier comparison notes: private corpus files 4815, 5790, 6031, 6183; probes 501, 503, 701.

### REL-5

**Rule.** A completed predecessor's finish lag is used up by the working time between its actual finish and the data date. A lag used up entirely adds nothing (it does not push the date to the next shift).

**Tests.** `blackbox_rules::a_completed_predecessors_finish_lag_is_used_up_by_the_time_since_it_finished`, `completed_boundaries::exhausted_actual_lag_does_not_snap_completed_predecessor_to_next_shift`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 525 of 86,356 corpus stored values, 190 of 19,624 corpus early dates, 48 of 129,622 probe fixtures. Cited in earlier comparison notes: private corpus files 4815, 5790, 6031, 6183.

### REL-6

**Rule.** Backward, every lag is counted in full, except under Retained Logic or Progress Override where a started predecessor's start-to-start lag is used up by its elapsed work as in the forward pass. A lag used up entirely leaves the date unchanged. A negative lag moves the date later onto a start moment.

**Tests.** `blackbox_rules::backward_lag_counts_in_full_except_a_started_start_ties_used_up_part`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 67 of 86,356 corpus stored values, 40 of 129,622 probe fixtures. Cited in earlier comparison notes: private corpus files 5772, 6031, 6035, 6321; probes 301-304, 504, 505, 508, 509. Documented option: Retained Logic, Progress Override and Actual Dates scheduling options.

### REL-7

**Rule.** A level-of-effort predecessor never drives its successors.

**Tests.** `blackbox_rules::level_of_effort_work_never_drives_its_successors`

**Evidence.** No stored output pins this rule. No fixture dates a successor of level-of-effort work: only the synthetic test pins it.

### REL-8

**Rule.** Under Progress Override, a started successor's start-side ties (finish-to-start and start-to-start) are replaced by its progress and give no candidate; its finish-side ties still hold. In Actual Dates mode the tie still applies.

**Tests.** `blackbox_rules::progress_override_ignores_start_side_ties_into_started_work_only`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 264 of 129,622 probe fixtures. Cited in earlier comparison notes: probes 302, 508. Documented option: Progress Override option.

### REL-9

**Rule.** Without Retained Logic, a completed predecessor still drives its not-started successor, from its actual finish (finish-side ties) or actual start (start-side ties) plus the whole lag. With Retained Logic it drives from its stored remaining dates.

**Tests.** `blackbox_rules::a_completed_predecessor_still_drives_from_its_actual_dates_without_retained_logic`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 132 of 129,622 probe fixtures. Cited in earlier comparison notes: probes 302, 303, 508, 509. Documented option: Retained Logic option.

### REL-10

**Rule.** A successor whose Expected Finish is in use gets no candidate from finish-side ties.

**Tests.** `blackbox_rules::expected_finish_discards_finish_side_ties`

**Evidence.** No stored output pins this rule. Documented option: Use Expected Finish Dates option. No stored output exercises the finish-side rule: only the synthetic test pins it.

### REL-11

**Rule.** A finish-side tie (finish-to-finish, start-to-finish) sets the earliest finish of the successor; its start follows by taking the successor's duration off that finish.

**Tests.** `blackbox_rules::a_finish_tie_sets_the_successors_finish_and_its_start_follows_from_the_duration`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 1,931 of 86,356 corpus stored values, 752 of 19,624 corpus early dates, 68 of 129,622 probe fixtures.

### REL-12

**Rule.** A successor starts at the latest of the start-side candidates and the converted finish-side candidates.

**Tests.** `blackbox_rules::an_activity_starts_at_the_latest_of_its_start_and_finish_driven_candidates`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 1,931 of 86,356 corpus stored values, 752 of 19,624 corpus early dates.

### REL-13

**Rule.** Under Progress Override, a start-side tie with no lag out of a started predecessor follows the data date rather than the predecessor's remaining start.

**Tests.** `blackbox_rules::under_progress_override_a_zero_lag_start_tie_from_started_work_follows_the_data_date`

**Evidence.** No stored output pins this rule. No stored output exercises this branch: only the synthetic test pins it.


### REL-14

**Rule.** The part of a completed predecessor's finish lag not yet used up (REL-5) does not move a completed successor: the successor keeps the predecessor's date. Ties between completed activities without lag still carry dates (a completed chain moved by open work stays together).

**Tests.** `parity_rules::a_completed_predecessors_unused_lag_does_not_move_completed_work`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions` (unchanged)

**Evidence.** Corpus stored values outside the oxdiff corpus: one export of the weekly set, where a completed activity is tied finish-to-finish with 50 h of lag into a completed successor and 40 h of the lag are left at the data date: the reference keeps the successor at the data date, where REL-5 alone would put it 40 h later. One example; the same file shows zero-lag ties between completed work carrying dates (two pairs). Breaking the rule changes 6 stored values in that file and none in the six comparisons.

## Backward pass

### BWD-1

**Rule.** A predecessor's finish is limited by the earliest of what its successors allow: a finish-to-start successor's late start (less the lag), a finish-to-finish successor's late finish (less the lag). A completed predecessor ignores the lags on its ties, except the unused finish lag of BWD-6.

**Tests.** `blackbox_rules::a_successors_late_dates_limit_its_predecessor_and_a_completed_predecessor_ignores_the_lag`, `blackbox_rules::start_side_limits_are_made_working_time_and_compete_with_finish_limits`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 1,808 of 86,356 corpus stored values, 16 of 129,622 probe fixtures. Cited in earlier comparison notes: private corpus file 369; probe 701.

### BWD-2

**Rule.** A tie limits a predecessor's finish (FS, FF) or start (SS, SF) by the successor's start (FS, SS) or finish (FF, SF). Summary successors never limit; level-of-effort successors do not when the check applies; a completed successor limits only when logic is retained; Progress Override gives no limit.

**Tests.** `blackbox_rules::the_limit_a_tie_gives_depends_on_its_type_and_ignores_summaries_and_finished_successors`

**Evidence.** No stored output pins this rule. Pinned by the synthetic test only; summary ties are covered by probe 601 (cited), which is not among the fixtures.

### BWD-3

**Rule.** A started successor offers its actual start (and a successor with an actual finish its actual finish) as the limit in Actual Dates mode, nothing under Progress Override, and its remaining dates under Retained Logic.

**Tests.** `blackbox_rules::a_started_successors_actual_start_stands_in_for_its_remaining_start`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 240 of 129,622 probe fixtures.

### BWD-4

**Rule.** The backward pass starts from the planned (must-finish) date when the option is on and one is set, else from the scheduled finish. Open ends take their early finish as late finish when that option is on. Late finish dates imposed by other projects hold unless external relationships are ignored.

**Tests.** `blackbox_rules::the_forward_and_backward_pass_give_textbook_dates_and_float`, `blackbox_rules::a_late_finish_imposed_by_another_project_holds_unless_external_relationships_are_ignored`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 3,847 of 86,356 corpus stored values, 824 of 129,622 probe fixtures. Documented option: Use planned (must-finish) date and open-ended activity options.

### BWD-5

**Rule.** The backward pass has no data-date clamp and no origin clamp: a late date may fall before the data date, giving negative float, and before the model's origin day, where it is stored as a negative minute. Scheduling (plain, leveled, or several projects together) gives the same results as the same project counted from an earlier origin.

**Tests.** `blackbox_rules::a_late_finish_before_the_data_date_gives_negative_float`, `blackbox_rules::zero_duration_work_has_equal_late_start_and_finish`, `blackbox_rules::late_dates_reach_before_the_origin_without_a_clamp`, `origin_room` (all tests in engine/tests/origin_room.rs)

**Evidence.** Not measured by breaking the rule. Stated by absence: there is no data-date clamp to remove. The synthetic test shows a late date before the data date and negative float; zero-duration late dates are pinned by the corpus. Before the origin the dates used to be clamped at the origin day (a Finish On or Before constraint 90 days before the data date, with the origin 42 days before it, gave late dates on the origin day and -30 days of float instead of -64); the stored-output comparisons call the passes directly and never reached before the origin, and no corpus file reruns.


### BWD-6

**Rule.** Backward, a completed predecessor's positive finish-to-start or finish-to-finish lag counts back only the part not yet used up: the working time on the lag calendar between its actual finish and the data date is taken off first, as in the forward pass (REL-5). This holds whichever way start-to-start lag is measured. Start-side ties and negative lags keep BWD-1 (the lag is ignored).

**Tests.** `parity_rules::backward_a_completed_predecessors_finish_lag_counts_only_its_unused_part`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions` (probe 304)

**Evidence.** Corpus stored values outside the oxdiff corpus: three exports of the weekly set. In one, a completed activity tied finish-to-finish with 50 h of lag, 40 h of it left at the data date, gets a late finish 40 h before its successor's; in another 50 h are left: 22 activities in the three files move, all to the reference's dates. Probe outputs: saved probes 1205 (lag from the early start) and 1206 (lag from the actual start) both apply the rule, and so do the stored late dates of saved probe 304 (lag from the actual start). An earlier version limited the rule to the early-start option to keep the probe-304 regression fixture exact; that fixture held an earlier implementation's values for completed work, which no gauge had compared with the reference, and its 16 differing cells now carry the reference's values stored in the saved probe.

### RES-1

**Rule.** When assignments are flagged as driving dates, an activity's finish from a start is the latest of the assignments' start plus remaining work plus lag, and a start from a finish the earliest of finish minus remaining work minus lag; unflagged assignments are ignored. Without driving assignments the activity's own remaining duration counts, and with none the date is unchanged.

**Tests.** `blackbox_rules::resource_driven_dates_use_the_longest_or_earliest_driving_assignment`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 6 of 129,622 probe fixtures.

### RES-2

**Rule.** A Resource Dependent activity works on its resources' calendars: forward, each assignment starts at the activity start moved to its resource's next working moment and runs its own duration; backward, each finishes at the late finish moved back to its resource's previous working finish; the activity spans earliest start to latest finish while its float is measured on its own calendar.

**Tests.** `probe_rules::resource_dependent_work_runs_on_the_resource_calendar`, `blackbox_rules::work_on_a_resource_calendar_and_activity_calendar_float_stay_apart`, `blackbox_rules::resource_dependent_work_spans_the_earliest_start_to_the_latest_finish_of_its_assignments`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 69 of 129,622 probe fixtures. Cited in earlier comparison notes: probes 814, 816.


## Boundary moments

### RES-3

**Rule.** Resource Dependent activities are placed on their resources' calendars (RES-2) by the plain schedule too, with leveling off. When no resource is over-allocated, a leveled run gives the same remaining dates, early and late dates and floats as the plain run.

**Tests.** `level_options::resource_dependent_work_is_scheduled_on_resource_calendars_without_leveling`, `level_options::saved_resource_dependent_probes_give_the_same_dates_with_and_without_leveling`

**Evidence.** No stored output. The reference was never run on a Resource Dependent probe without leveling; the saved probes 814 and 816 are leveled runs. The rule is the RES-2 placement applied without the leveler, checked against the leveled answer where the leveler adds nothing.

### RES-4

**Rule.** A completed activity counted on round-the-clock time (PASS-3) moves its dates by an assignment's remaining duration as day numbers: the moment as days since 1899-12-30 plus the fraction of its day, plus hours / 24 with the hours exactly units / units per hour, rounded to the nearest minute with ties to even. A duration ending on an exact half minute therefore rounds up from some moments and down from others, as the binary value falls. Elsewhere durations are whole minutes (units / units per hour rounded half-even).

**Tests.** `parity_rules::round_the_clock_hours_are_added_as_day_numbers`

**Evidence.** Corpus stored values outside the oxdiff corpus: one completed activity of the weekly set (0.38 units left at 0.16 per hour, 142.5 minutes) in 13 of its exports: 09:22 after Monday 07:00 in 6, 19:23 after Friday 17:00 in 7; the day-number arithmetic gives every one. Saved probe 1203 gives 11:22 after 09:00, also as computed. Every other fractional-minute assignment in the 69 exports of both sets (295 half-minute and 8,101 other open ones) matches with whole minutes.

### EDGE-1

**Rule.** An early finish moment stays when the minute before it is working time (a finish at a shift's close stays); otherwise it moves on to the next working start. An early start moment is the next working start. A late start moment stays when it is working time, otherwise it is the previous working finish. A late finish moment is the previous working finish.

**Tests.** `blackbox_rules::a_finish_keeps_its_working_boundary_and_a_start_after_a_non_work_minute_moves_on`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 214 of 86,356 corpus stored values, 128 of 19,624 corpus early dates, 312 of 129,622 probe fixtures.

### EDGE-2

**Rule.** A start milestone's late date follows the late-start rule and a finish milestone's early date follows the early-finish rule (the end each one has).

**Tests.** `blackbox_rules::a_milestone_takes_the_date_of_the_end_it_has_when_the_boundary_rules_apply`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 82 of 86,356 corpus stored values, 12 of 129,622 probe fixtures.


## Forward pass

### FWD-1

**Rule.** A zero-duration open task or start milestone starts at the next working start whether or not a tie drives it; an open finish milestone keeps a finish moment at a shift's close; a completed activity keeps its raw date even outside working time.

**Tests.** `blackbox_rules::zero_duration_work_starts_at_the_next_working_start_but_a_finish_milestone_stays`, `completed_boundaries::completed_finish_milestone_keeps_data_date_outside_working_hours`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 13 of 86,356 corpus stored values, 16 of 19,624 corpus early dates. Cited in earlier comparison notes: private corpus file 4815; probes 000, 506-507, 510-512, 702-704, 1001-1006. Four sub-clauses change no stored output and no test result: the extra snap of a driven start milestone (the zero-duration clause snaps it again), the reset of a milestone's start to its finish, the exemption of completed work (which sits on round-the-clock time and cannot move) and the milestone finish rule.

### FWD-2

**Rule.** Work never starts before the data date or its resume date. An early start imposed by a relationship from another project holds unless external relationships are ignored.

**Tests.** `blackbox_rules::work_never_starts_before_the_data_date_or_its_resume_date_or_a_start_imposed_from_another_project`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 10 of 129,622 probe fixtures. Cited in earlier comparison notes: probes 405, 815, 817 (resume dates). Documented option: Ignore relationships to and from other projects option; Retained Logic suspend/resume handling. The data-date floor is taken twice in the code; the second is redundant.

### FWD-3

**Rule.** In Actual Dates mode an actual start after the data date holds the early start; in Retained Logic it does not.

**Tests.** `blackbox_rules::an_actual_start_after_the_data_date_holds_the_start_in_actual_dates_mode_only`

**Evidence.** No stored output pins this rule. Documented option: Actual Dates option. No stored output exercises an actual start after the data date: synthetic test only.

### FWD-4

**Rule.** When no working time lies between an activity's finish and the limit of the finish-to-finish or start-to-finish tie driving it, the finish is that limit. Only finish-side ties do this.

**Tests.** `blackbox_rules::a_finish_in_a_gap_is_extended_to_the_driving_finish_ties_limit`, `completed_boundaries::zero_work_finish_gap_respects_admin_setting`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 2 of 86,356 corpus stored values, 2 of 19,624 corpus early dates, 72 of 129,622 probe fixtures.

### FWD-5

**Rule.** The backward mirror: when a late start lies after the start-side limit of the driving start-to-start or start-to-finish tie and no working time lies between them, the late start is the limit (a start a tie places at a shift's close stays there).

**Tests.** `probe_rules::backward_start_stays_at_a_ties_shift_close`, `blackbox_rules::a_late_start_at_a_start_ties_shift_close_is_kept_by_the_backward_pass`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 4 of 129,622 probe fixtures. Cited in earlier comparison notes: probe 701.


## Constraints

### CON-1

**Rule.** Start On, Start On or After, Finish On and Finish On or After move the early dates (a finish constraint through the duration); Start On or Before, Finish On or Before and As Late As Possible do not.

**Tests.** `blackbox_rules::start_constraints_hold_the_early_start_and_finish_constraints_hold_the_early_finish`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 117 of 86,356 corpus stored values, 24 of 19,624 corpus early dates, 299 of 129,622 probe fixtures. Documented option: Constraint type semantics (No Earlier Than, No Later Than, On, Mandatory, As Late As Possible).

### CON-2

**Rule.** Start On, Start On or Before, Finish On and Finish On or Before cap the late dates; Start On or After and Finish On or After do not.

**Tests.** `blackbox_rules::before_constraints_cap_the_late_dates_and_after_constraints_do_not`

**Evidence.** No stored output pins this rule. Documented option: Constraint type semantics. The late-date caps are pinned by the synthetic test; no stored output distinguishes the zero-duration variant.

### CON-3

**Rule.** An Expected Finish replaces the date of a finish constraint (not a start constraint).

**Tests.** `blackbox_rules::an_expected_finish_replaces_the_date_of_a_finish_constraint`

**Evidence.** No stored output pins this rule. Documented option: Use Expected Finish Dates option. No stored output exercises an Expected Finish replacing a constraint date: only the synthetic test pins it.

### CON-4

**Rule.** A start milestone ignores finish constraints and a finish milestone ignores start constraints.

**Tests.** `blackbox_rules::a_milestone_ignores_constraints_on_the_end_it_does_not_have`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 240 of 129,622 probe fixtures.

### CON-5

**Rule.** Early constraints bind only work not yet started. Late start constraints bind only unstarted work; late finish constraints bind anything not completed.

**Tests.** `blackbox_rules::constraints_bind_only_work_that_has_not_reached_their_end`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 2,697 of 86,356 corpus stored values, 2,092 of 19,624 corpus early dates.

### CON-6

**Rule.** A start constraint on a zero-duration start milestone in non-working time rolls back to the previous working finish; for work with a duration the late finish is the duration after the rolled start.

**Tests.** `blackbox_rules::a_start_milestone_with_a_start_constraint_in_non_working_time_rolls_back_to_the_previous_finish`

**Evidence.** No stored output pins this rule. Cited in earlier comparison notes: private corpus file 6215, but no fixture cell moves when the clause is broken: synthetic test only.

### CON-7

**Rule.** Mandatory constraints are placed before the passes: a Mandatory Start in non-working time starts at the next working start, a Mandatory Finish converts to a start through the duration, a mandatory constraint on the end a milestone lacks gives no dates; backward, the constraint date is the late finish. This placement covers work not started; work in progress is left to the passes unless CON-9's switch pins it.

**Tests.** `probe_rules::mandatory_start_converts_to_a_late_finish_through_the_duration`, `probe_rules::mandatory_start_on_a_finish_milestone_leaves_it_undated`, `blackbox_rules::mandatory_dates_are_placed_before_the_passes_and_move_on_from_non_working_time`, `blackbox_rules::mandatory_dates_never_precede_the_data_date_and_keep_a_finish_milestones_finish`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 22 of 129,622 probe fixtures. Cited in earlier comparison notes: probe 201. Documented option: Mandatory constraint semantics. Three details change no stored output and no test result: the branch for completed work (never reached, because the early reader gives nothing for work already started), the follow-up Expected Finish step, and a second boundary adjustment.

### CON-8

**Rule.** A mandatory primary constraint on work not started gives equal early and late dates and zero total float (work in progress: CON-9), the activity has zero free float, and the tie into a mandatory successor never drives (the secondary constraint does not count). This holds when every activity the passes schedule is mandatory too (a single Mandatory Finish, or everything complete but a final mandatory milestone): the dates the pre-pass placed are stored and the project finish follows them.

**Tests.** `blackbox_rules::a_mandatory_constraint_gives_equal_early_and_late_dates_and_zero_free_float`, `blackbox_rules::a_schedule_of_mandatory_work_only_still_stores_its_dates`, `completed_boundaries::primary_mandatory_successor_constraint_suppresses_relationship_driving`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 50 of 129,622 probe fixtures.

### CON-9

**Rule.** An activity in progress whose primary constraint is Mandatory Start or Mandatory Finish is scheduled by the forward and backward passes like any started activity: it gets early, late and remaining dates and a total float, and its successors wait for its remaining work, under Retained Logic and Progress Override alike. Where the mandatory date places it is **UNVERIFIED** (see Evidence): the engine applies CON-5 as for every other constraint, so the early dates run from the data date over the remaining duration by logic; a Mandatory Finish still caps the late finish (a late finish constraint binds anything not completed) and can give negative float; a Mandatory Start binds nothing. Zero free float and the never-driving tie into it (CON-8) still apply. Completed mandatory work keeps the CON-7 handling.

**Tests.** started_mandatory (all tests in engine/tests/started_mandatory.rs: Mandatory Finish, Mandatory Start, and a Mandatory Finish before the work can finish, each under Retained Logic and Progress Override)

**Evidence.** No reference output. Settled without one: work that is not complete must get dates and float and its finish-to-start successors must wait for it (before this rule the activity got no early or remaining dates and no total float, a started Mandatory Start activity no late dates, and its successors started at the data date). Not settled: whether the reference pins started work to the mandatory date as it pins work not started (CON-7, CON-8) or schedules it by logic. A reference run was attempted and not completed; saved probes 1401 to 1404 (Mandatory Finish and Mandatory Start on started work, each under Retained Logic and Progress Override) are pending. CON-5 was followed because it is the existing rule for constraints on work in progress, the constraint readers the mandatory pre-pass uses already apply it to the mandatory types, and the leveler already treats started mandatory work as unpinned (no leveler probe is known to contain such work). The other reading is implemented behind the single switch `MANDATORY_PINS_STARTED_WORK` in `model.rs` (`true` pins started work like work not started); the tests carry the expected dates for both readings, so the probe answer is a one-line change. A corpus file or fixture holding a started mandatory activity with stored early dates would already have differed before this rule (the engine gave it none), so the comparisons, which were exact, cannot settle the question either; none of the six moves with the change.


## Expected Finish

### EXP-1

**Rule.** With Use Expected Finish on, an activity's remaining duration runs from its start to its expected finish; an expected finish at or before the start leaves no remaining work (unless an assignment drives dates); a finish-side tie driving the activity leaves the duration alone; the option off ignores the date. As Late As Possible work and leveling do not use it.

**Tests.** `blackbox_rules::an_expected_finish_sets_the_remaining_duration_from_the_start`, `blackbox_rules::expected_finish_is_not_used_by_alap_work_or_while_leveling`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 1,526 of 86,356 corpus stored values, 174 of 19,624 corpus early dates, 5 of 129,622 probe fixtures. Documented option: Use Expected Finish Dates option. One clause (dropping a constraint date when an actual start precedes the expected finish) is reachable only for an activity marked not started that carries an actual start; no fixture and no test exercises it.

### EXP-2

**Rule.** An Expected Finish is a finish moment: one at a shift's start or in non-working time rolls back to the previous working finish.

**Tests.** `probe_rules::expected_finish_at_a_shift_start_rolls_back_to_the_previous_finish`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 12 of 129,622 probe fixtures. Cited in earlier comparison notes: probe 402.


## Float and the longest path

### FLT-1

**Rule.** Forward and backward passes give the textbook early and late dates, total float is the working time from early to late dates, the project finish is the latest early finish, and the critical chain has zero float.

**Tests.** `blackbox_rules::the_forward_and_backward_pass_give_textbook_dates_and_float`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`, `oxdiff engine engine/fixtures/features`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 7,561 of 86,356 corpus stored values, 20,924 of 129,622 probe fixtures, 122 of 424 feature fixtures. Documented option: Textbook forward/backward pass and float definitions. Textbook definition; the project finish and the pass results are pinned by every corpus project.

### FLT-2

**Rule.** Free float is the smallest gap, on the activity's own calendar, between its finish (or start) and each successor's start (or finish) after taking the tie lag off on the lag calendar; with no successor it runs to the project finish. Level-of-effort work, work with an Expected Finish in use and mandatory-constrained work have none. A started activity's start-side ties count the lag as the forward pass does.

**Tests.** `blackbox_rules::free_float_is_the_gap_to_the_nearest_successor_after_the_lag_and_open_ends_run_to_the_finish`, `relationship_candidates::independent_relationship_candidates_and_negative_lag_milestones`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 81 of 86,356 corpus stored values, 1,066 of 129,622 probe fixtures. Cited in earlier comparison notes: private corpus file 5772; probes 302, 303. Four checks change no stored output: level-of-effort work returning zero (the run then sets zero anyway), skipping summary and level-of-effort successors (undated when float is computed), and, for started work, the reach of a start lag and its consumption (corpus: one cell).

### FLT-3

**Rule.** A tie drives when its free float is zero.

**Tests.** `blackbox_rules::a_ties_driving_flag_is_set_when_its_free_float_is_zero`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`, `oxdiff engine engine/fixtures/features`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 4,804 of 86,356 corpus stored values, 80 of 129,622 probe fixtures, 12 of 424 feature fixtures. Documented option: Definition of a driving (zero free float) relationship. Also pinned by the relationship answers (1,500 ties) together with FLT-9.

### FLT-4

**Rule.** Ties of summary activities never drive; ties of level-of-effort activities have zero free float and always drive.

**Tests.** `blackbox_rules::ties_of_summaries_never_drive_and_ties_of_level_of_effort_work_always_do`

**Evidence.** No stored output pins this rule. Cited in earlier comparison notes: probe 601. The zero-float clause for level-of-effort ties changes no stored output (the value is zero anyway in every fixture); summary ties cited from probe 601.

### FLT-5

**Rule.** Progress replaces a tie when the matching ends have actual dates: FS by a predecessor finish or successor start, FF by either finish, SF by predecessor start or successor finish, SS by either start. Such ties are not counted in free float under Progress Override.

**Tests.** `blackbox_rules::progress_replaces_a_tie_when_either_end_has_the_matching_actual_date`, `blackbox_rules::progress_override_leaves_ties_that_progress_replaces_out_of_free_float`; stored-output comparisons: `oxdiff engine engine/fixtures/regressions`

**Evidence.** Probe outputs: breaking the rule on purpose changes 32 of 129,622 probe fixtures. Cited in earlier comparison notes: probes 302, 508.

### FLT-6

**Rule.** The float type option chooses start float, finish float, or the smaller; in Actual Dates and Progress Override modes an activity with an expected finish reports its finish float.

**Tests.** `blackbox_rules::the_float_type_chooses_start_float_finish_float_or_the_smaller`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 280 of 86,356 corpus stored values, 88 of 129,622 probe fixtures. Documented option: Float type option (Start, Finish, Smallest). The two non-default float types are documented option semantics; the corpus uses the smallest-float setting. An equality shortcut in finish float is redundant.

### FLT-7

**Rule.** The longest path ends at the open activities (level of effort included, summaries excluded) whose early finish equals the latest early finish of the open non-LOE work, and runs back through driving ties only, completed predecessors included, in a leveled run as well (one export of the daily set). Each project of a multi-project run has its own.

**Tests.** `blackbox_rules::the_longest_path_is_the_chain_of_driving_ties_back_from_the_last_finish`, `parity_rules::the_leveled_longest_path_runs_back_into_completed_work`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`, `oxdiff engine engine/fixtures/features`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 1,581 of 86,356 corpus stored values, 216 of 129,622 probe fixtures, 14 of 424 feature fixtures. Cited in earlier comparison notes: all 14 unleveled private corpus projects. The walk back through driving ties and the choice of the last finish are pinned by the corpus. Breaking any single filter (skipping level-of-effort ends when finding the last finish, excluding summaries, open work only) changes no stored flag: those filters are pinned by the synthetic test.

### FLT-8

**Rule.** An As Late As Possible activity moves later by its free float and then has none; a start milestone moves to the start edge; predecessors' free float is recomputed.

**Tests.** `blackbox_rules::as_late_as_possible_work_moves_to_use_its_free_float`, `fresh_schedule::alap_consumes_free_float_once`; stored-output comparisons: `oxdiff engine`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 21 of 86,356 corpus stored values. Cited in earlier comparison notes: private corpus file 6183. Documented option: As Late As Possible constraint semantics. The activity case is pinned by the corpus; the move is applied twice (before and after float), and removing either application, or the free-float recomputations, changes no result.

### FLT-9

**Rule.** A tie's free float and driving flag by scheduling mode: Actual Dates measures from the predecessor's actual date (else its remaining date) plus the whole lag to the successor's actual date (else its remaining date); a mandatory predecessor gives zero and does not drive; Progress Override gives zero for a tie the ends' progress replaces; completed ends without remaining dates give zero (driving only under Retained Logic); under Retained Logic it is measured from the predecessor's remaining date plus the effective lag to the successor's remaining start.

**Tests.** `probe_rules::ties_from_completed_work_by_scheduling_mode`, `relationship_candidates::independent_relationship_candidates_and_negative_lag_milestones`, `blackbox_rules::tie_float_by_mode_follows_the_ends_actual_and_remaining_dates`, `blackbox_rules::a_tie_out_of_a_mandatory_activity_gives_zero_float_and_drives_only_when_logic_is_retained`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 523 of 86,356 corpus stored values, 80 of 129,622 probe fixtures. Cited in earlier comparison notes: probes 201, 301-304, 508, 509. The pair-availability clause (remaining dates count as present when either exists) and several mode details are pinned by the synthetic test; the corpus-free parts are pinned by probe outputs.


## Pass bookkeeping

### PASS-1

**Rule.** A work breakdown summary's dates are one point: the data date moved to a start moment; its remaining duration is zero, its total float zero and its free float runs from that point to the project finish.

**Tests.** `blackbox_rules::a_work_breakdown_summary_is_a_single_point_at_the_data_date`

**Evidence.** No stored output pins this rule. Cited in earlier comparison notes: probes 601, 815. Documented option: Summary activity semantics. Synthetic test and probes 601, 815 (cited); no fixture cell moves when the summary rules are broken.

### PASS-2

**Rule.** A not-started activity's planned (target) dates are its scheduled early dates; started activities keep theirs.

**Tests.** `blackbox_rules::not_started_work_takes_its_planned_dates_from_the_schedule`

**Evidence.** No stored output pins this rule. Pinned by the synthetic test only.

### PASS-3

**Rule.** A completed activity keeps no remaining dates, expected finish or float; unless logic is retained it keeps no early or late dates either. Under Retained Logic or Progress Override a completed activity is placed on round-the-clock time.

**Tests.** `blackbox_rules::completed_work_keeps_no_scheduled_values_unless_logic_is_retained`, `completed_boundaries::completed_finish_milestone_keeps_data_date_outside_working_hours`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 9,666 of 86,356 corpus stored values, 582 of 19,624 corpus early dates, 14,380 of 129,622 probe fixtures. The round-the-clock placement of a completed summary changes no result.

### PASS-4

**Rule.** Open work is scheduled; completed work is scheduled only when logic is retained.

**Tests.** `blackbox_rules::only_open_work_is_scheduled_unless_logic_is_retained`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/regressions`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 11,967 of 86,356 corpus stored values, 8,432 of 19,624 corpus early dates, 14,432 of 129,622 probe fixtures. Documented option: Retained Logic option.

### PASS-5

**Rule.** Level-of-effort and summary activities take no part in the forward and backward passes.

**Tests.** `blackbox_rules::level_of_effort_and_summary_activities_take_no_part_in_the_passes`; stored-output comparisons: `oxdiff engine`, `oxdiff forward`, `oxdiff engine engine/fixtures/features`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 34 of 86,356 corpus stored values, 24 of 19,624 corpus early dates, 8 of 424 feature fixtures. Skipping level-of-effort work is pinned by the corpus; skipping summaries is pinned by the synthetic test.

### PASS-6

**Rule.** A level-of-effort activity tied start-to-start to one activity and finish-to-finish to another spans the first's early start to the second's early finish; its remaining duration is the working time of that span on its own calendar; its late dates sit on working moments of its own calendar; its free float is zero and its total float is computed as for any activity.

**Tests.** loe_rules (all tests in engine/tests/loe_rules.rs), `blackbox_rules::an_open_level_of_effort_has_zero_free_float_and_the_float_of_any_activity`; stored-output comparisons: `oxdiff engine`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 12 of 86,356 corpus stored values. Cited in earlier comparison notes: private corpus files 369, 4815 and 6031; probes 510-512. A guard keeping the remaining duration from going negative changes nothing.

### PASS-7

**Rule.** Results do not depend on the order activities are listed in.

**Tests.** `blackbox_rules::input_order_does_not_change_any_result`

**Evidence.** No stored output pins this rule. Documented option: Order independence follows from CPM itself. The order in which ready activities are processed is not observable in any stored value or test: reversing the seeding order or loosening the readiness check changes nothing. Order independence is a property of CPM, pinned by the permutation test.

### PASS-8

**Rule.** A fresh schedule discards remaining dates from an earlier run, runs the forward pass, backward pass, as-late-as-possible, float, level of effort, planned dates and summaries, and stores remaining dates equal to the early dates (remaining late dates equal to the late dates).

**Tests.** `fresh_schedule::fresh_cpm_replaces_imported_remaining_dates`, `fresh_schedule::remaining_dates_are_early_not_late`, `blackbox_rules::a_fresh_schedule_stores_remaining_late_dates_equal_to_the_late_dates`; stored-output comparisons: `oxdiff engine`, `oxdiff engine engine/fixtures/regressions`, `oxdiff engine engine/fixtures/features`

**Evidence.** Corpus stored values: breaking the rule on purpose changes 13,372 of 86,356 corpus stored values, 5,138 of 129,622 probe fixtures, 48 of 424 feature fixtures. Discarding imported remaining dates at the start, restoring completed work's finish before ALAP, and running summaries before planned dates change no result (later steps overwrite them).

### PASS-9

**Rule.** Several projects scheduled as one network keep each project's own data date, late limit and finish, store the limit that relationships to other projects give, and have a longest path each.

**Tests.** multi_project (all tests in engine/tests/multi_project.rs)

**Evidence.** Not measured by breaking the rule. Pinned by tests/multi_project.rs (synthetic projects).


### PASS-10

**Rule.** A loop that closes only through level-of-effort or summary activities is not a circular relationship: those activities take no part in the passes (PASS-5), so the network is scheduled. A loop through ordinary work is refused and named without them.

**Tests.** `parity_rules::a_loop_that_closes_only_through_a_level_of_effort_is_scheduled`

**Evidence.** Corpus stored values outside the oxdiff corpus: 16 consecutive exports of the daily set, where a level of effort is tied backwards (a later milestone finish-to-finish into it, and it start-to-start into an earlier milestone), closing a loop through ordinary work only by way of the level of effort. The reference scheduled every one; with the loop accepted, all early and late dates of all 16 match.

## Global setting

### ADM-1

**Rule.** With the global finish-on-valid-time administrator setting on, a finish-side candidate that falls on a day's first work start (a negative-lag finish-to-finish or nonpositive-lag start-to-finish tie) is moved to the previous working finish, and so is a finish extended across a gap with no work (FWD-4) when the limit is a day's first work start. A lunch boundary is not rolled back. The default is off.

**Tests.** `completed_boundaries::negative_finish_boundary_uses_admin_setting_independently_of_retained_logic`, `completed_boundaries::zero_work_finish_gap_respects_admin_setting`, `completed_boundaries::admin_setting_does_not_roll_back_lunch_boundary`

**Evidence.** Not measured by breaking the rule. Documented option: Global administrator setting for finishing FF relationships on a valid time. The default (off) is pinned by the corpus and the benchmark. The on branch has no stored output: it is a documented administrator option, pinned by the synthetic tests only.


## Leveling and added features

### LVL-1

**Rule.** Serial, non-splitting resource leveling that preserves the scheduled early and late dates, on a merged-interval view of each calendar that starts two days before the origin.

**Tests.** `schedule_features::native_leveling_matches_saved_probes`, `schedule_features::native_leveling_matches_full_saved_schedule`

**Evidence.** Not measured by breaking the rule. Cited in earlier comparison notes: probes 801-817; the two leveled private corpus files, 369 and one other. Evidence is the independently captured leveling answers of the earlier comparison runs (301 of 301 remaining date pairs and 72 of 72 float and driving pairs on probes 801-817, and the leveled benchmark). Not re-measured here.

### LVL-2

**Rule.** Leveled tie free float uses the full lag at leveled dates.

**Tests.** `schedule_features::native_leveling_matches_saved_probes`

**Evidence.** Not measured by breaking the rule. Cited in earlier comparison notes: probes 809-813. As LVL-1.

### LVL-3

**Rule.** When leveling is limited to float, the leveler delays an activity only to a start whose finish still leaves at least the float to preserve before the activity's late finish, counted in working time on the activity's calendar. If the first free slot would leave less, the activity stays where its logic puts it, the resource stays over-allocated there, and the activity is reported. The float to preserve is read only in this mode. Early and late dates do not change.

**Tests.** `level_options::leveling_without_the_float_option_delays_past_the_float`, `level_options::within_float_an_activity_is_delayed_while_its_float_allows`, `level_options::within_float_an_activity_without_enough_float_stays_and_is_listed`, `level_options::preserving_two_days_of_float_keeps_two_days`, `level_options::the_float_to_preserve_is_read_only_when_leveling_within_float`, `level_options::a_delay_follows_the_late_dates_down_a_chain`

**Evidence.** No stored output. Level Resources was never run on a project with this option, and no corpus file has it switched on. The rule is what the option and the preserved float mean in the scheduler's public help; the choice to leave an unmovable activity in place rather than move it past its float is ours.

### LVL-4

**Rule.** The resources chosen for leveling are the only ones placed against their limits; any other resource is booked against no limit. Choosing every resource gives the same schedule as leveling all resources; choosing none levels nothing.

**Tests.** `level_options::leveling_all_resources_equals_selecting_every_resource`, `level_options::a_resource_outside_the_selection_is_not_leveled`

**Evidence.** Corpus stored values cannot tell this rule from its absence (the one corpus file with a selection never has an unselected resource over its limit). The rule is the meaning of the option.

### LVL-5

**Rule.** When leveling is limited to float, activities that tie on every leveling priority (total float included) are placed in order of their internal activity id (creation order); otherwise the tie is broken by Activity ID. Data-fitted. Saved probe 1202 (option off) confirms the Activity ID tie-break; probe 1201 (option on) has to be run again, because the reference scheduled it with relationships and a calendar left over from an earlier probe (its stored output shows ties the probe does not have).

**Tests.** `parity_rules::leveling_within_float_breaks_ties_by_internal_id_not_activity_id`

**Evidence.** Corpus stored values outside the oxdiff corpus: two consecutive exports of the daily set, the only two leveled within float. With the internal-id tie-break every leveled date of both files matches (every open activity of both files); with the Activity ID tie-break 881 and 1,954 activities differ. Seven other tie-breaks were tried (leveled early start, late start, late finish, float left at the leveled start, remaining duration either way, CPM early start, each before, instead of or after total float) and all were worse. Applying the internal-id tie-break to the other 15 leveled exports of the set breaks them, so it depends on the option.

### LVL-6

**Rule.** When leveling is limited to float, the project's scheduled finish becomes the latest leveled finish of the open work, and the leveled longest path (FLT-7) ends at open work whose early finish equals that finish; when leveling moved the finish past every early finish, no activity is flagged. Without the option the scheduled finish stays the latest early finish.

**Tests.** `parity_rules::leveling_within_float_moves_the_project_finish_to_the_leveled_finish`

**Evidence.** Corpus stored values outside the oxdiff corpus: one export of the daily set stores a scheduled finish 16.5 h after its latest early finish, equal to the latest leveled finish, and no longest-path flag at all. In the other 18 leveled exports of the set the stored finish is the latest early finish even where leveling ends later (in one, the latest leveled finish is almost five days after the latest early finish). One file shows the moved finish; the gating on the option rests on that contrast.

### FEAT-1

**Rule.** A level-of-effort activity whose ties are not the single start-to-start / finish-to-finish shape spans its tied activities' earliest start to latest finish, late dates on its own calendar.

**Tests.** schedule_features::feature_fixture_differential (fixtures/features/loe-*.txt)

**Evidence.** Not measured by breaking the rule. No reference output exists. The fixtures under engine/fixtures/features were produced by an earlier implementation of this engine: they test agreement with it, not with the reference.

### FEAT-2

**Rule.** Multiple float paths and activity critical paths are produced in a fixed order and never change dates or critical flags.

**Tests.** activity_paths (engine/tests/activity_paths.rs), schedule_features::feature_fixture_differential (fixtures/features/paths-*.txt)

**Evidence.** Not measured by breaking the rule. No reference output exists. The order of float paths and the tie breaking are design choices pinned only by agreement with the earlier implementation's fixtures.


### FEAT-3

**Rule.** After leveling, an open level-of-effort activity outside the PASS-6 shape takes its remaining dates from its tied activities' leveled dates by the FEAT-1 endpoint rule (earliest start-side candidate, latest finish-side candidate, lags on the lag calendar, actual dates where its partners have them), whatever the direction of its ties.

**Tests.** `parity_rules::after_leveling_a_level_of_effort_spans_its_partners_leveled_dates`

**Evidence.** Corpus stored values outside the oxdiff corpus: in the daily set, the backward-tied level of effort of PASS-10 (its remaining finish equals its partner milestone's leveled finish) in 14 exports, and a level of effort tied finish-to-finish with -15 h of lag (its remaining finish 15 h after its partner's leveled finish) in one export. These also confirm FEAT-1's early and late dates for the backward-tied shape, which until now had no reference output.

## For review: rules without output evidence

Rules that no stored output pins (breaking them leaves all six comparisons
exact). They stay as code with a synthetic test:

REL-1, REL-7, REL-10, REL-13, BWD-2, FWD-3, CON-2, CON-3, CON-6, FLT-4, PASS-1, PASS-2, PASS-7.

Rules with no reference output to compare with at all: CON-9 (probes 1401 to 1404 pending), LVL-3, LVL-4, RES-3, FEAT-1, FEAT-2 (agreement with an earlier implementation only), PASS-9 (multi-project scheduling, synthetic tests only) and the on branch of ADM-1.

Rules pinned by the calendar fixtures only:

CAL-2, CAL-3, CAL-4, CAL-5, CAL-6, CAL-7, CAL-8, CAL-9.

The calendar fixtures (`engine/fixtures/cal.txt`) give the expected result of
each calendar operation; whether those answers were taken from saved
reference probes or computed by an earlier implementation is not recorded
in the repository. The scheduling passes use only the position functions
(`work_at`, `start_at`, `finish_at` and the snapping built on them), which the
corpus does pin (CAL-10, CAL-9). The helper functions in CAL-2 to CAL-8 are not
used by the passes.

Code kept without any evidence, output or test:

- `cpm.rs` forward pass, Expected Finish: dropping a constraint date when an actual start precedes the expected finish (EXP-1). Reachable only for work marked not started that carries an actual start.
- `cpm.rs` forward pass, zero duration (FWD-1): the extra snap of a driven start milestone, the reset of a milestone's start to its finish, and the completed-work exemption. Completed work sits on round-the-clock time, so the exemption cannot matter.
- `scheduler.rs` mandatory pre-pass (CON-7): the branch for completed work is never reached (the early reader returns nothing for work already started), the follow-up Expected Finish step after placing a mandatory date, and a second boundary adjustment in the backward step.
- `cpm.rs` free float (FLT-2): the zero return for level-of-effort work, the skip of summary and level-of-effort successors, and the reach of a started activity's start lag.
- `cpm.rs` finish float (FLT-6): the shortcut returning zero when early and late finish are equal (the difference is already zero).
- `scheduler.rs` longest path (FLT-7): the filters that skip level-of-effort ends when finding the last finish, exclude summaries from the walk, and keep completed work out of the end set.
- `scheduler.rs` as-late-as-possible (FLT-8): the second application after float, and the recomputation of predecessor float.
- `scheduler.rs` pass bookkeeping (PASS-3, PASS-6, PASS-8): the completed summary calendar, the non-negative guard on a level-of-effort remaining duration, the discard of imported remaining dates (overwritten at the end), restoring a completed activity's finish after the backward pass, and the order of planned dates against summaries.
- `scheduler.rs` ordering (PASS-7): the reverse seeding order and the extra readiness check. Not observable; kept as is.
- `cpm.rs` forward pass (FWD-2): the data-date floor is applied twice.

Options the engine stores but no rule reads: `use_remaining_dates`.

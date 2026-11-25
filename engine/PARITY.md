# Parity with stored reference results

How the engine's results compare with the results the reference scheduler
stored in real schedule files and in saved probe projects. The inputs hold
client data and stay outside the repository; the tests that read them skip
with a printed reason when they are absent (`AGENTS.md`). Each rule named
here is stated, with its own evidence, in `RULES.md`.

## What was compared

- **Private corpus.** 18 XER files. 16 were written by the reference
  scheduler and hold 4,873 activities; the other 2 were written by a
  generator. Each file is imported, scheduled afresh with its own stored
  options (leveled when the file was leveled), written back, and 11 stored
  TASK cells per activity are compared with the file's own: early, late,
  remaining and remaining late start and finish, total float, free float
  and the longest-path flag. Every status is compared, in whole minutes,
  with no tolerance. The check is `xer/tests/corpus_parity.rs`.
- **Saved probe projects.** 58 small projects, each built to isolate one
  behavior and scheduled by the reference: every stored TASK field of 8,602
  open activities, plus the free float and driving flag of 4,989
  relationships.
- **A rescheduled project.** One project of 4,617 activities, scheduled by
  OpenXER and compared like for like with the reference's own reschedule
  after one inserted activity: 17 TASK cells per activity (dates, actual
  dates, floats, durations and status). The check is
  `automation/tests/local_reference.rs` (ignored, local files only).
- **Update histories.** 69 consecutive exports of two projects (20 daily
  updates, leveled; 49 weekly updates, not leveled), compared field by
  field over TASK and TASKRSRC with a fresh schedule of each export.

## What matched

| Comparison | Matching |
| --- | ---: |
| Corpus, files with current stored results | 50,479 / 50,479 cells |
| Saved probes, open activity fields | every field of all 58 probes |
| Saved probes, relationship free float and driving | 4,989 / 4,989 each |
| Rescheduled project, like for like | every cell of 4,617 activities |
| Update histories, daily set | 19 of 20 exports |
| Update histories, weekly set | 44 of 49 exports |

The corpus totals split as 10,615 cells in clean files, 1,914 in files
edited after their last schedule without a scheduling input changing, and
37,950 in leveled files.

The differential fixtures that `oxdiff` compares are derived from these
sources and must stay exact: cal 77,620, forward 19,624, engine 86,356,
roundtrip 17, regressions 129,622, features 424.

## Residual differences

None of these is an engine difference.

- **Stale stored results.** An activity was marked complete after the last
  schedule run, so the file stores no dates for it and the stored results of
  its neighbours describe the earlier inputs (3 corpus files and 4 weekly
  exports). Restoring the activity's earlier status makes the file match.
- **Edited after the last schedule run.** A remaining duration or other
  input was changed between the schedule run and the export, or between
  the schedule run and leveling (1 weekly and 1 daily export).
- **Generated files.** 2 corpus files were written by a generator, not by
  the reference, so their dates are not a reference answer. One has
  malformed calendar text and does not import.

## Open rules without reference evidence

These rules are implemented and pinned by synthetic tests, but no stored
reference output confirms them (`RULES.md`, "For review"):

- CON-9: started work under a Mandatory Start or Mandatory Finish
  (UNVERIFIED; saved probes 1401 to 1404 are pending).
- LVL-3 and LVL-4: leveling only within float, and leveling selected
  resources against a resource that is over its limit.
- LVL-5 with the option on: the tie-break is data-fitted on two exports; its
  confirming probe has to be run again.
- RES-3: Resource Dependent activities scheduled without leveling.
- FEAT-1 and FEAT-2: general level-of-effort shapes and the order of
  multiple float paths.
- PASS-9: several projects scheduled as one network.
- ADM-1 with the administrator setting on.
- Not modeled: "Max percent to over-allocate" is stored but has no effect,
  and a Resource Dependent activity whose assignments have no units or no
  readable resource calendar stays on its own calendar.

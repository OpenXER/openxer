# Add New Baseline

Creates a new baseline for the open project. A baseline is a frozen copy of the project that later progress is measured against. Open it with the **Add** button of [Maintain Baselines](maintain-baselines) (`Project > Maintain Baselines...`).

## What is on it
- **What would you like to do?**: pick one of two ways to make the baseline.
- **Save a copy of the current project as a new baseline**: freezes the open project as it is now, with its current dates, durations, units and costs. **OK** makes it at once.
- **Convert another project to a new baseline of the current project**: turns a different workspace project into a baseline of this one. After **OK** you pick that project in [Select Project](select-baseline-project).
- **OK**: carries out the choice.
- **Cancel**: closes without adding a baseline.

## Common tasks
1. Press `F9` so the dates you freeze are current.
2. Choose `Project > Maintain Baselines...` and click **Add**.
3. Keep **Save a copy of the current project as a new baseline** and click **OK**.
4. The new baseline appears in the list, selected. Rename it in **Baseline Name**.
5. Assign it in [Assign Baselines](assign-baselines).

## Good to know
> A copy gets the ID "<project ID> - B1" and the name "<project name> - B1", then B2 and so on, taking the lowest free number. Its Last Update Date is the time it was taken. The status bar says "Baseline saved".
> A converted project keeps its own ID and name; its Last Update Date stays blank until the baseline is updated.
> Converting moves the other project out of the workspace project list: it becomes a baseline and is no longer a project of its own.
> An open project cannot be converted; close it first. A project that has baselines of its own cannot be converted either.
> Adding a baseline is one undo step.
> Baselines are kept in the workspace beside the project. If the project is not saved in the workspace yet, OpenXER saves it first.
> To send baselines with the project, tick them for that project in the [Export Wizard](export-wizard).

## See also
- [Maintain Baselines](maintain-baselines)
- [Select Project](select-baseline-project)
- [Assign Baselines](assign-baselines)

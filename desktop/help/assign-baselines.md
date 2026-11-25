# Assign Baselines

Chooses which baselines a project is compared against. A baseline is a frozen copy of the project; assigning one makes its dates and values show in the BL columns and the baseline bars. Open it with `Project > Assign Baselines...`.

## What is on it
- **Project**: the open project to assign baselines for, shown as "ID : name". With several projects open, pick one; OK makes it the current project.
- **Project Baseline**: the main baseline of the project. Click the box or its **...** button to pick one of the project's baselines. It shows **<Current Project>**, the project's own planned dates, when none is assigned.
- **User Baselines**: three more slots, **Primary**, **Secondary** and **Tertiary**, each picked the same way. A new project starts with Primary on **<Current Project>** and the other two on **<None>**.
- **The baseline lookup** (opened by **...**): the project's baselines by name. **Select** puts the selected one in the field, **Remove** sets the field back to **<Current Project>** (Project Baseline, Primary) or **<None>** (Secondary, Tertiary), and **Close** leaves it as it is.
- **OK**: keeps the choices. A slot that now points at a stored baseline also turns on its baseline bar in the Gantt chart.
- **Cancel**: closes without changes.

## Where the baselines are used
- The project baseline feeds the **BL Project** columns of the activity table and Check Schedule (Late Activities and BEI), and its name shows in the status bar.
- The Primary, Secondary and Tertiary baselines feed the BL1, BL2 and BL3 columns.
- The Gantt chart's baseline bars and the progress line read the assigned baselines.
- The project baseline choice is saved with the project and goes out in its XER file. The Primary, Secondary and Tertiary choices are kept in the workspace only.

## Common tasks
1. Create a baseline first in [Maintain Baselines](maintain-baselines).
2. Choose `Project > Assign Baselines...`.
3. Click **...** beside **Project Baseline**, click the baseline and click **Select**.
4. Set **Primary** to the same baseline if your layouts use BL1 columns or bars.
5. Click **OK**.

## Good to know
> Baseline values are matched to the project's activities by Activity ID, so keep IDs stable after you take a baseline.
> A baseline's bars use its planned dates and its duration is its original duration. The BL start and finish columns use the planned dates too, unless the earned value setting in [Admin Preferences](admin-preferences) is set to the baseline's current dates.
> The choice is one undo step.
> An assigned baseline cannot be deleted or restored in Maintain Baselines; assign another one first.

## See also
- [Maintain Baselines](maintain-baselines)
- [Bars](bars)
- [Columns](columns)
- [Check Schedule](check-schedule)

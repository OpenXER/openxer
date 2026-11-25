# Select Project

Picks the workspace project to turn into a baseline of the open project. A baseline is a frozen copy that progress is compared against. It opens when you choose **Convert another project to a new baseline of the current project** in [Add New Baseline](add-baseline) and click **OK**.

## What is on it
- **The project list**: the workspace projects, with **Project ID** and **Project Name**. Click one to select it. When there are more projects than fit, the list scrolls with the mouse wheel or its scroll bar.
- **Select**: converts the selected project into a new baseline of the open project. It is greyed out until a project is selected.
- **Close**: closes without converting.

## Which projects can be converted
- A project that is open (the current project or another open one) cannot become a baseline. OpenXER says so and the list stays open; close the project first.
- A project that has baselines of its own cannot become a baseline either.

## Common tasks
1. Choose `Project > Maintain Baselines...` and click **Add**.
2. Pick **Convert another project to a new baseline of the current project** and click **OK**.
3. Click the project to convert, for example an earlier revision you imported from an XER file.
4. Click **Select**. The new baseline appears in Maintain Baselines, selected, with the project's own ID and name, and the status bar says "Project converted to a baseline".

## Good to know
> The converted project leaves the workspace project list. It now exists only as a baseline of the open project.
> Activities are matched to the baseline by Activity ID, so a revision that kept the same IDs compares best.
> The conversion is one undo step: `Ctrl+Z` brings the project back.

## See also
- [Add New Baseline](add-baseline)
- [Maintain Baselines](maintain-baselines)
- [Assign Baselines](assign-baselines)
- [Import Wizard](import-wizard)

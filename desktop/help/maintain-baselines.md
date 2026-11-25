# Maintain Baselines

Manages the baselines of the open projects. A baseline is a frozen copy of a project, kept in the workspace beside it, that you compare progress against. Open it with `Project > Maintain Baselines...`.

## What is on it
- **The baseline list**: a band for each open project with its baselines under it. Columns: **Project ID/Baseline ID**, **Baseline Name**, **Data Date**, **Date Added** and **Last Update Date**.
- **Baseline ID** and **Baseline Name**: type a new ID or name; it is saved when you leave the box, pick another row or close the dialog. Neither can be blank.
- **Baseline Type**: **<None>** or one of the baseline types defined on the **Baseline Types** tab of `Admin > Categories and Lists...`, listed by name. It is saved as soon as you pick it.
- **Data Date** and **Last Update Date**: read only.
- **Add**: opens [Add New Baseline](add-baseline) for the selected project.
- **Delete**: deletes the selected baseline after you confirm, then says it is done.
- **Copy**: makes another baseline of the same project from the selected one, with the next " - B" number.
- **Update...**: opens [Update Baseline](update-baseline) to bring changes from the project into the baseline.
- **Restore**: after you confirm, takes the selected baseline out of the list and makes it a separate project with the same ID and name, in the same project folder as its project.
- **Close**: closes the dialog.

## Common tasks
1. Choose `Project > Maintain Baselines...`.
2. Click **Add**, keep **Save a copy of the current project as a new baseline** and click **OK**.
3. Select the new row and give it a clear **Baseline Name**, such as "Original contract".
4. Click **Close**, then pick it in [Assign Baselines](assign-baselines) so the BL columns and baseline bars use it.

## Good to know
> **Delete**, **Copy**, **Update...** and **Restore** need a baseline selected; with a project band selected only **Close**, **Add** and **Help** work.
> A baseline in use as the project baseline or as a primary, secondary or tertiary baseline cannot be deleted or restored; OpenXER says so. Assign another one first in [Assign Baselines](assign-baselines).
> A new copy is named after the project with " - B1", " - B2" and so on, taking the lowest free number. A copy keeps the data date, type and last update date of the baseline it came from.
> Restore cannot use a Project ID another project already has; change the **Baseline ID** first.
> To roll the open project itself back to a baseline, use `Project > Revert Project to Baseline...` ([Revert Project to Baseline](revert-to-baseline)).
> With several projects open, a command on another project's band makes that project the current one first.
> When there are more baselines than fit, the list scrolls: use the mouse wheel or the scroll bar on its right.
> Every change here is one undo step.

## See also
- [Assign Baselines](assign-baselines)
- [Add New Baseline](add-baseline)
- [Update Baseline](update-baseline)
- [Revert Project to Baseline](revert-to-baseline)
- [Schedule Comparison](schedule-comparison)

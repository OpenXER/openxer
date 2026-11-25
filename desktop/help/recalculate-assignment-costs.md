# Recalculate Assignment Costs

Reprices the resource assignments of every open project from the resources' prices, as one undo step. With several projects open, the status bar says how many projects were recalculated, and skipped assignments are listed with their project. Use it after you change a resource's price or rates. Open it with `Tools > Recalculate Assignment Costs...`.

## What is on it
- **All resources on project(s)**: recalculates every assignment. This is the default.
- **Active resources only**: leaves out assignments of resources that are not active. They are not counted as skipped.
- **Synchronize Overtime Factor while recalculating cost**: copies each resource's overtime factor onto its assignments before the costs are worked out. A resource with no overtime factor leaves the assignment's factor as it is. Off by default.
- **Recalculate**: runs the recalculation and closes the dialog.
- **Close**: closes without changes.

## How the costs are worked out
For each assignment, OpenXER takes the price per unit from the resource rate in effect at the assignment's start, using the assignment's rate type, and writes it as the assignment's price per unit. Then:
- budgeted cost is budgeted units times the price;
- actual regular cost is actual regular units times the price;
- actual overtime cost is actual overtime units times the price times the overtime factor (a blank factor counts as 1);
- remaining cost is remaining units times the price.

## Common tasks
1. Change the price on the resource.
2. Choose `Tools > Recalculate Assignment Costs...`.
3. Pick **All resources on project(s)** and click **Recalculate**.
4. Read the status bar: it gives how many assignments were updated, unchanged and skipped.

## Good to know
> Some assignments are skipped and left as they are: those priced from a role or an override, those whose costs are not linked to units, those whose resource has no price or whose price changes during the assignment, and those without a resource.
> The skipped assignments and the reason for each are listed in the log. Open it with **View Log** in the Schedule dialog.
> The recalculation is one undo step. When nothing changes, no undo step is made.
> To reprice after every run, turn on **Recalculate assignment costs after scheduling** in Schedule Options, or the matching box in Level Resources.

## See also
- [Schedule Options](schedule-options)
- [Level Resources](level-resources)
- [Resources view](view-resources)
- [Assignments view](view-assignments)

# Delete or Merge Resources

Removes a resource from the resource dictionary, or merges it into another resource. Merging is the usual clean-up after importing XER files from several sources, when one crew or machine arrives under two or three Resource IDs. Open it in the Resources view: right-click a resource and choose `Del. / Merge`.

## What is on it
- **Delete the resource**: removes it with its rates, roles, codes and field values. Only a resource with no assignments and no child resources can be deleted.
- **Merge it into the resource picked below**: moves everything that names it to the resource you click in the list, then removes it.
- **Merge too**: tick other duplicates in the list to merge them into the same resource in one step.
- **The list**: resources of the same type (labor, nonlabor or material). Click a row to make it the resource everything moves to. A resource with the same name is picked for you when there is one.
- **The preview box**: what will move where, before anything changes.
- **Merge** or **Delete**: applies the change as one step that `Edit > Undo` takes back.
- **Cancel**: closes without changing anything.
- **Help**: opens this topic.

## What a merge moves
- Assignments in every project of the file, and the primary resource of activities.
- Rate rows, role links, resource codes and resource field values. Where the target already has a rate from the same date, the same role, a value of the same code or of the same field, the target's own entry is kept and the other is dropped.
- Child resources move under the target. A target without a calendar takes the first merged resource's calendar.
- Leveling lists and any other table that names the resource.

## Good to know
> When the target and a merged resource are both assigned to one activity, the two assignments become one: budgeted, actual and remaining units and costs are added, units per time are added, the dates span both, and the target's own assignment keeps its role, rate type, price and curve. The preview lists the activities this affects.
> Stored prices and costs are not recalculated. Run `Tools > Recalculate Assignment Costs...` afterwards if you want the target's prices applied.
> Reschedule with `F9` after a merge so the dates and the usage profiles reflect the combined assignments.
> The same edit is available to scripts and AI agents as the `merge_resources` and `delete_resource` operations.

## See also
- [Resources view](view-resources)
- [Recalculate Assignment Costs](recalculate-assignment-costs)

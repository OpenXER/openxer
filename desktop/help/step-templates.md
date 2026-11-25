# Activity Step Templates

Steps break an activity into smaller pieces of work, such as set, align and grout, each with a weight that counts toward the activity's progress. A step template is a named list of steps you can reuse on many activities. Open this dialog with `Enterprise > Activity Step Templates...` or from the Dictionaries toolbar.

## What is on it
- **Templates** list:
- **Step Template Name**: the template's name, up to 60 characters and unique. Click it on the selected row to rename it in place.
- **Steps**: how many steps the template holds.
- **Steps** list, for the selected template:
- **Step Name**: click it on the selected row to rename the step. It cannot be blank.
- **Step Weight**: a number of 0 or more. A step's share of the activity is its weight divided by the total of all the weights.
- **Close**: closes the dialog.
- **Add**: adds a template named New Template at the end of the list.
- **Delete**: removes the selected template and its steps, after asking.
- **Add Step**: adds a step named New Step with a weight of 1 at the end of the selected template.
- **Delete Step**: removes the selected step at once, without asking.
- **Help**: opens this topic.

## Common tasks
1. Click **Add**, then click the new template's name and type a name for it.
2. Click **Add Step** once for each step, then rename each one and set its **Step Weight**.
3. On an activity's Steps tab, use **Add from Template** to copy the template's steps onto the activity. With a step selected they go in right after it; with none, after the last step.
4. To give several activities the same steps at once, select them in the activity table (Ctrl+click or Shift+click) and click **Steps** on the Assign toolbar. Pick the template and click **Add**: every selected activity gets its steps, after its last step (after the selected step on the activity shown in Activity Details). `Ctrl+Z` takes them all back in one step.

## Good to know
> Adding a template to an activity copies its steps. Later changes to the template do not change activities that already have the steps.
> Deleting a template leaves the steps it already added to activities in place.
> You can also build a template from an activity's own steps with **Create Template...** on the right-click menu of the activity's Steps tab.
> Templates are part of the project data, so they travel with the XER file.
> The Steps tab can show a **Step Weight Percent** column (right-click the steps, **Customize Steps Columns...**): each step's weight as a percent of all the activity's step weights, with one decimal. It is worked out for you and cannot be typed.

## See also
- [Activities view](view-activities)
- [Update Progress](update-progress)

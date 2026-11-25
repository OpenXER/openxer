# New Activity Wizard

The New Activity wizard walks you through a new activity's fields, one page at a time. It opens from `Edit > Add`, the **Add** button, the `Insert` key and the row menu's **Add** while **Add activities through the wizard** is on in [User Preferences](user-preferences), Assistance (it is on until you turn it off).

## What is on it
- **Activity Name**: the next Activity ID from the project's Auto-numbering Defaults and the name "New Activity", both ready to type over. Only the Activity ID is required: it may not be blank or used by another activity in the project. The name may be left blank.
- **Work Breakdown Structure**: the WBS element the activity goes in; it starts with the one Add would use (the selected activity's).
- **Activity Type**: starts with the project's Defaults tab value.
- **Assign Resources**: **Assign** opens Assign Resources; each resource you pick joins the list. **Remove** takes the selected one off. A milestone takes no resources.
- **Duration Type**: starts with the project's Defaults tab value.
- **Units and Duration**: **High Labor Units** and **High Nonlabor Units** (hours, 0 to start with) become the activity's budgeted units, and **High Duration** (5 days to start with) its Original Duration.
- **Dependent Activities**: **Yes** adds two pages, **Predecessor Activities** and **Successor Activities**. On each, **Assign** opens Assign Predecessors or Assign Successors (with the relationship type and lag) and **Remove** takes one off the list. **No** (the default) goes on.
- **More Details**: **Yes** adds three pages: **Expenses** (**Add** puts in an expense, whose name and budgeted cost you type beside the buttons; **Delete** takes it off), **Activity Codes** (**Assign** opens Assign Activity Codes; one value per code) and **Work Products and Documents** (**Assign** opens Assign Work Products and Documents). **No** is the default.
- **Congratulations**: the last page.
- **Skip this wizard next time**: turns the wizard off when it closes, so the next Add puts the row in at once.

## Buttons
- **Cancel** at the bottom left closes the wizard and adds nothing, as does `Escape`.
- **Prev** and **Next** move between the pages; `Enter` is **Next**, and **Finish** on the last page.
- **Finish**, on any page, adds the activity with what you have chosen so far; the rest keep the project's defaults.

## Good to know
> The activity, its resource assignments, relationships, expenses, codes and documents are one step: `Ctrl+Z` takes all of them back at once.
> A blank or used Activity ID stops **Next** on the first page and **Finish** with a message; after **OK** the wizard shows the page the message is about. Nothing is added until it is fixed.
> With the wizard off, Add puts the new row in at once under the selected activity's WBS and opens its name for typing. Turn the wizard back on in [User Preferences](user-preferences), Assistance.
> Scripts and AI agents add activities with `add_activity` and have no wizard; `openxer-cli workspace preference` reads or sets the wizard switch of a workspace.

## See also
- [Activities](view-activities)
- [Assign Predecessors and Successors](assign-relationships)
- [User Preferences](user-preferences)

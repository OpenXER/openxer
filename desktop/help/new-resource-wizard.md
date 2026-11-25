# New Resource Wizard

The New Resource wizard walks you through a new resource's fields, one page at a time. It opens from **Add** in the Resources view (the Edit bar, the row menu and `Insert`) while **Add resources through the wizard** is on in [User Preferences](user-preferences), Assistance (it is on until you turn it off).

## What is on it
- **ID and Name**: "R" (or the next free ID, such as "R-1", when "R" is taken) and "(New Resource)" to start with, the same as Add without the wizard, ready to type over. Only the Resource ID is required: it may not be blank or used by another resource.
- **Resource Type**: **Labor (people)** (the default), **Nonlabor (machines, equipment)** or **Material**. **Unit of Measure** is open for a material only and picks from Admin Categories, Units of Measure.
- **Units / Time & Prices**: **Price/Unit** (0 to start with), **Default Units/Time** and **Max Units/Time** (8 hours a day to start with), in your units format.
- **Phone & E-Mail**: **Office Phone** and **E-Mail Address**.
- **Roles**: **Assign** opens Roles to pick from; the first role is the primary role. **Remove** takes the selected one off.
- **Resource Calendar**: use a calendar that already exists (the default), or make the resource a personal calendar.
- **Select Existing Calendar**: the calendar (the project's calendar to start with); the field opens Calendars to choose another, and **View/Edit** opens Calendars. With a personal calendar this page shows the name it will get, the Resource ID and name.
- **Auto-Compute Actuals**: computed for you (the default), or left to you.
- **Congratulations**: the last page.
- **Skip this wizard next time**: turns the wizard off when it closes, so the next Add puts the resource in at once.

## Buttons
- **Cancel** at the bottom left closes the wizard and adds nothing, as does `Escape`.
- **Prev** and **Next** move between the pages; `Enter` is **Next**, and **Finish** on the last page.
- **Finish**, on any page, adds the resource with what you have chosen so far; the rest keep the defaults.

## Good to know
> The resource, its rate, roles and personal calendar are one step: `Ctrl+Z` takes all of them back at once.
> A blank or used Resource ID stops **Next** on the first page and **Finish** with a message; after **OK** the wizard shows the first page.
> The personal calendar is made when you click **Finish**, copied from the calendar chosen on the Select Existing Calendar page. Cancel makes none.
> Scripts and AI agents add resources with `add_resource` and have no wizard; `openxer-cli workspace preference` reads or sets the wizard switch of a workspace.

## See also
- [Resources](view-resources)
- [User Preferences](user-preferences)
- [New Activity Wizard](new-activity-wizard)

# Shared Dictionaries

A workspace keeps its dictionaries once for all of its projects: global calendars, resources and their rates, roles, global activity codes, EPS activity codes, user defined fields, cost accounts, responsible managers, currencies, curves, shifts, funding sources and the admin categories. A calendar or resource made while one project is open is offered in every other project at once, and a change to it reaches every project that uses it. Project calendars, project activity codes, the WBS, activities, relationships and assignments stay with their own project.

## How it works
- **Edits reach the other projects right away.** When an edit changes a shared row (a calendar's work hours, a resource's limits, a new global code), the other open projects take the change in the same step, and projects opened later see it too.
- **Undo takes back the step in every project.** `Edit > Undo` of the step puts back the shared rows that step changed, and only those; changes other projects made since are kept.
- **A row in use stays.** Deleting a shared calendar, resource, code or field that another project still uses is refused; the message names the projects that use it.
- **EPS codes follow the EPS.** An EPS activity code is offered in the projects of its EPS node and of the nodes below it. An EPS code that comes with an XER file is filed under the importing project's node.
- **Rollups stay current.** The Projects view refreshes the rollups of closed projects when a shared calendar or resource they use changes.

## Share Dictionaries Across Projects (Admin menu)
A workspace made with an earlier version of OpenXER keeps a copy of every dictionary inside each project. `Admin > Share Dictionaries Across Projects...` converts it; the command is unavailable once a workspace shares its dictionaries, and new workspaces share them from the start. When you open an older workspace, the status bar says once where the command is. Nothing is ever converted without you.
- **The project list**: for each project, how many of its dictionary rows are the same as another project's (**Shared**), new to the workspace (**Added**), given a new id because theirs was taken (**Renumbered**), and kept as a renamed copy because a different row has the same name (**Renamed copies**).
- **What changes**: one line per renamed copy, with the fields that differ, and per renumbered row.
- **Apply**: first copies the workspace into the `backups` folder beside it, converts it in one step and checks that every project's dates and float are the same as before. If any check fails, nothing is changed. The open projects are opened again afterwards.
- **Cancel** (or Escape): closes without changing anything. **Help**: opens this topic.

## Importing into a workspace
When an XER file is imported, its calendars, resources, codes and other dictionary rows are matched to the workspace's by name:
- The same name and the same content: the workspace's row is used, nothing is added. Importing the same file twice adds no duplicates.
- The same name and different content: by default the file's row is added as a copy named `<name>_1` (or the next free number), and the import report lists it with the fields that differ. On the wizard's **Dictionaries** step you can choose per kind of row to **Keep** the workspace's row instead, or to **Update** the workspace's row with the file's values (every project that uses it then sees the new values).
- No match: the row is added.
An exported XER file holds only the shared rows its project uses.

## Good to know
> A conversion cannot be taken back with `Edit > Undo`; the backup copy keeps the workspace as it was. Open it with `OpenXER --database <backup file>` to look at it.
> Scripts and AI agents use the same dictionary: `openxer-cli workspace dictionary` lists it, `workspace dictionary-usage` says which projects use a row, and `workspace migrate` converts a workspace file.

## See also
- [Calendars](calendars)
- [Activity, Project, Resource and Assignment Codes](codes)
- [Projects view](view-projects)
- [Import wizard](import-wizard)

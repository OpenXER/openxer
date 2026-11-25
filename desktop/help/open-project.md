# Open Project

Picks which workspace projects are open. Choose `File > Open...` or press `Ctrl+O`.

## What is on it
- **Project ID** and **Project Name**: every project in the workspace, filed under its EPS folders (the folder tree of the Projects view). A project that is already open shows "(open)" after its ID. When the window opens, the open projects are selected.
- **+/- box** on a folder row: shows or hides what the folder holds, without selecting anything.
- **Open**: opens the selected projects. It is available once at least one project is selected.
- **Cancel**: closes the window and changes nothing.
- **Help**: opens this topic.
- **File...**: opens an XER file from disk instead, through a file picker. If the file holds more than one project, you choose which one next.

## Common tasks
1. Press `Ctrl+O`.
2. Click a project to select it alone. `Ctrl`+click adds or removes one more project; `Shift`+click selects a run of rows.
   Click a folder to select every project under it, including the projects in its child folders. `Ctrl`+click a folder to add its projects to the selection, and again to take them out.
3. Click **Open**. Double-click a project to open just that one, or a folder to open everything under it.

To open a whole branch, for example every job of one region, click its folder, then **Open**. To open every project, click each top folder with `Ctrl` held, then **Open**.

## Good to know
> The selection is the whole open set: a project that is open now but not selected when you click **Open** is closed.
> Unsaved changes are saved before the projects change. If the save fails, nothing changes.
> Undo history that spans projects is cleared when the open set changes. A project you had open on its own keeps its undo history for the session: open it again and `Ctrl+Z` still undoes its steps, including projects you added, pasted or deleted in the Projects view while it was open.
> The project that is active now stays active when it is among those opened; otherwise the first selected project in the list becomes active. The default project stays the same when it is still open, else the active project becomes the default.
> With several projects open, the activity table and the Gantt chart show them together, and new activities go to the default project.

## See also
- [Set Default Project](default-project)
- [New Project](new-project)
- [Import wizard](import-wizard)
- [Projects view](view-projects)

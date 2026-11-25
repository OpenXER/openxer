# Project Folders (EPS)

The EPS (enterprise project structure) is the folder tree that holds your projects. Use this dialog to build and arrange that tree. Open it with `Enterprise > Project Folders (EPS)...`.

## What is on it
- **EPS ID**, **EPS Name**: the folder's short ID and full name. Click a cell of the selected row to edit it in place; Enter keeps the change, Escape drops it. IDs must be unique in the whole tree, names unique among folders at the same level.
- **Responsible Manager** column: the manager set for the folder, shown read-only.
- **Responsible Manager** drop-down (under the list): picks the manager for the selected folder from the open project's Responsible Managers (OBS). Choose the blank entry to clear it. It is unavailable while the project has no OBS elements.
- **Close**: closes the dialog.
- **Add**: adds "New EPS Node" as the last folder under the selected one.
- **Delete**: deletes the selected folder with everything under it: the folders below it and every project filed in any of them, with those projects' baselines. One question first counts the items that go (folders plus projects); **No**, the default, keeps everything.
- **Cut**, **Copy**, **Paste**: move or duplicate a folder with its sub-folders. Paste puts it under the selected folder. A copy gets new IDs, and a number is added to any name that clashes.
- **Arrow pad**: up and down move the folder among its neighbours; right makes it the last child of the folder above it at the same level; left moves it out one level.
- **Help**: opens this topic.
- Right-click a row for Add, Delete, Cut, Copy and Paste.

## Common tasks
1. Select the folder that should hold the new one and click **Add**.
2. Click the new row's **EPS Name** cell, type the name and press Enter.
3. Pick a **Responsible Manager** under the list.
4. Use the arrow pad to put the folder where it belongs.

## Good to know
> The EPS is not stored in XER files. It lives in your OpenXER workspace and spans all your projects.
> Deleting a folder deletes the projects in it. To keep a project, move it to another folder first (cut it in the Projects view and paste it on that folder).
> An open project in the deleted folder is saved and closed first; when the current project goes, another open project becomes current (or the first project outside the folder opens). If every project of the workspace is in the folder and one is open, Delete is refused, because one project always stays open.
> Edit > Undo (Ctrl+Z) brings the folders, the projects and their baselines back in the same session; the projects come back closed. The Projects view's **Delete** on a folder row does the same.
> To file a project in a folder, cut it in the Projects view and paste it on the folder, or choose the folder in the New Project dialog or the Import wizard.
> AI agents can list and edit the folders too (`openxer-cli eps`), with the same rules; deleting a folder that holds projects needs `--delete-projects` there.
> The tree must keep at least one folder, and it can be at most 10 levels deep.

## See also
- [Responsible Managers (OBS)](responsible-managers)
- [Projects view](view-projects)

# Getting started

This page walks you through a first session: bring a project in, look around,
make a change, schedule, and send the file back out.

## The sample project
A new workspace opens on **RIVERBEND**, a made-up process water upgrade in
progress: about 190 activities with logic, three calendars, crews and
equipment with rates, costs, expenses, codes, notebook topics and an
as-planned baseline. Use it to try the views. To remove it, and the
calendars, resources, codes and notebook topics only it uses, delete it in
the Projects view once another project is open. See [Projects](view-projects).

## Bring a project in
1. To start from an XER file, use `File > Import...`. The wizard reads the file
   into your workspace. See [Import wizard](import-wizard).
2. To reopen a project that is already in the workspace, use `File > Open...`
   (`Ctrl+O`). See [Open Project](open-project).
3. To start from nothing, use `File > New...` (`Ctrl+N`). See
   [New Project](new-project).

## The main window
- The **menu bar** and **toolbars** run commands. Hover a toolbar button to see
  its name.
- The **Directory** bar at the left switches between views: Projects, WBS,
  Activities, Resources, Reports and the others.
- The **Activities** view is where most of the work happens: the activity table at
  the left, the bar chart at the right, and the activity details tabs below. See
  [Activities](view-activities).
- Views open as tabs. You can split them into tab groups from `View > Tab Groups`.
- The **status bar** at the bottom shows **Modified, saved on close** while an open
  project has unsaved changes (closing OpenXER or switching projects saves them;
  `File > Save` saves them now), the default project when several are open, the data date, the project baseline,
  the user, the workspace file and messages from the last command.

## Make a change and schedule
1. Click an activity in the table. Its details show in the tabs below.
2. Type a new value into a cell or a details field. Every edit is one undo step:
   `Ctrl+Z` undoes it and `Ctrl+Y` redoes it.
3. Press `F9` to open the Schedule dialog and click **Schedule** to recalculate
   the dates. See [Scheduling basics](scheduling-basics).

## Save and send the file out
- `Ctrl+S` or `F10` saves your changes to the workspace.
- `File > Export...` writes an XER file for someone else. See
  [Export wizard](export-wizard).

## Good to know
> The workspace is a database file beside the program (or in your local
  application data folder when that folder cannot be written). Saving there is
  not the same as writing an XER file: export when you need to hand the schedule
  to someone.
> Every dialog closes with `Escape`. **Cancel** leaves the project as it was. A
  **Close** button often keeps what you changed (Level Resources and Spell
  Check do; Schedule Options asks first); each dialog's topic says which.

## See also
- [Keyboard shortcuts](keyboard-shortcuts)
- [Columns](columns), [Filters](filters) and [Group and Sort](group-and-sort) to
  change what the table shows.

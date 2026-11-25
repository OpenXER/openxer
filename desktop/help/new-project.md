# New Project

Makes an empty project in the workspace and opens it. Choose `File > New...`, press `Ctrl+N`, or click **Add** in the Projects view.

## What is on it
- **EPS**: the folder in the project tree (the EPS) the new project goes under. It starts on the folder selected in the Projects view, or the one that holds the open project. Opened from **Add** in the Projects view, it starts on the selected folder, or on the folder of the selected project.
- **Project ID**: the short code of the project, up to 20 characters. It starts on the first free code such as NEW-1. It must not match another project in the workspace.
- **Project Name**: the long name, up to 100 characters. Left blank, the name is the Project ID.
- **Planned Start**: the date work can begin. It starts on today. A date typed without a time starts at 08:00. The data date (the "as of" date of the schedule) starts here too.
- **Must Finish By**: an optional target finish. A date without a time means 17:00 that day. It cannot be before the Planned Start.
- **Responsible Manager**: the person or group answerable for the project. It starts on the manager of the open project; pick the blank row for none.
- **Default Rate Type**: which of the five prices (**Price / Unit** to **Price / Unit5**) new resource assignments of this project use. It starts on **Price / Unit** and only applies to assignments added later; you can change it on the **Resources** tab of Project Details.
- **Finish**: checks the entries, adds the project and opens it in the Activities view.
- **Cancel**: closes the window and adds nothing.

## Common tasks
1. Press `Ctrl+N`.
2. Pick the **EPS** folder and type a **Project ID** and **Project Name**.
3. Type the **Planned Start** and, if there is a deadline, **Must Finish By**.
4. Click **Finish**, then add activities in the Activities view. The new project's **Defaults** tab starts at Fixed Duration & Units, Duration percent complete, Task Dependent, Activity IDs from A1000 by 10, incremented from the selected activity.

## Good to know
> A problem with an entry shows in red above the buttons, and the window stays open until it is fixed.
> The new project has one WBS row (the top of its work breakdown) named after the project, the standard calendar and no activities.
> Adding the project is one undo step, and it is the first step of the new project's own undo history. `Ctrl+Z` right after **Finish** takes the project back out and returns you to the project you were in; `Ctrl+Y` there adds it again and opens it.
> AI agents can make a project the same way, with the same checks: `openxer-cli eps create-project` (see Working with AI agents).

## See also
- [Open Project](open-project)
- [Project Folders](project-folders)
- [Responsible Managers](responsible-managers)
- [Getting started](getting-started)

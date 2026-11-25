# External Applications

Keeps a short list of other programs you can start from OpenXER, such as a spreadsheet, a report tool or your own script. Choose `Enterprise > External Applications...`. Start a listed program from `View > Run Program`.

## What is on it
- **The list**: each program you have added, with its name and command line. Click one to load it into the boxes below. When the list is empty it says "No applications are listed."
- **Name**: the name shown on the `View > Run Program` menu.
- **Command Line**: the program to start: a full path to the program file, or a program name your computer can find on its own.
- **Arguments**: extra words passed to the program, separated by spaces. Put double quotes around a group of words that belongs together, such as a path with spaces.
- **Add**: adds the boxes as a new entry.
- **Change**: saves the boxes over the selected entry.
- **Delete**: removes the selected entry.
- **Close**: closes the window.

## Passing the schedule to a program
Type `%xer%` in **Arguments** and OpenXER writes a fresh XER file of the open project just before the program starts, then passes that file's path in place of `%xer%`. The file holds the schedule as it is on screen, unsaved changes included. Temporary files more than a day old are removed the next time a program that uses `%xer%` is started.

## Common tasks
1. Choose `Enterprise > External Applications...`.
2. Type a **Name**, such as "Open in my checker".
3. Click in **Command Line** and type the path to the program, such as `C:\Tools\checker.exe`.
4. Type `%xer%` in **Arguments**.
5. Click **Add**, then **Close**.
6. Choose `View > Run Program` and click the new name.

## Good to know
> The program starts directly, not through a command window, so command-window tricks such as pipes and wildcards do not work. The command line cannot hold < > | * or ?.
> Each name must be different (letter case does not count) and at most 60 characters, and the list holds at most 30 programs.
> When the command line is a path, Add and Change check that the file exists.
> If the program cannot be found or started, a message says so.
> The list is kept in the workspace, so it is there for every project.

## See also
- [Export](export-wizard)
- [Customize](customize-toolbars)
- [Working with AI agents](working-with-ai-agents)

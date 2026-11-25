# Working with AI agents

An AI agent (an assistant such as Claude Desktop, the ChatGPT desktop app, Codex or
Claude Code) can work on your schedule files through OpenXER's own tool server,
which runs on this PC. The `AI Agents` menu sets this up for you. When OpenXER
finds one of these apps and none is connected yet, the status bar says so; click
it to open **Connect AI Agents**.

## What is on the menu
- **Connect AI Agents...**: one row per app, each with the one thing to do (see
  below).
- **Connection Status**: shows which apps are set up and whether each can reach
  OpenXER.
- **Chat** (`F8`): ask an agent about the open schedule in a window beside your
  work ([Chat](ai-chat)).
- **Copy Setup for Other Agents**: copies the MCP settings another agent needs, to
  paste into its own configuration.
- **Skills...**: the instructions agents follow to check and build schedules;
  change them or write your own ([Skills for AI agents](skills-for-ai-agents)).
- **What Can Agents Do?**: a short list of what an agent can do and things to ask.

## The rows of Connect AI Agents
- **Connect**: the app is on this PC and not connected yet.
- **Connected**, with a tick: right after connecting, OpenXER tests the
  connection and shows "Working" with the number of tools, or what went wrong.
  **Test** runs it again.
- **Use This Copy**: the app is connected to another copy of OpenXER on this PC
  (for example an installed copy and a portable one). It works, so OpenXER leaves
  it alone until you click this.
- **Repair**: the app points at a copy of OpenXER that is no longer there.
- **Not installed**: the app is not on this PC.
- **Disconnect** takes OpenXER out of an app again.
- **Connect All** connects every app found at once, when there is more
  than one. It never switches an app that works with another copy.

Each change backs up the app's settings file first and changes nothing in it but
OpenXER's entry.

## Common tasks
1. Open `AI Agents > Connect AI Agents...` and click **Connect** next to your app.
2. Do what the dialog says next. Claude Desktop: quit it from its icon by the
   clock (closing the window is not enough), then open it again. ChatGPT desktop
   and Codex share one setting, so one Connect serves both: restart the app, and in
   ChatGPT desktop use Work or Codex mode (for now, plain Chat may not use tools on
   this PC). Claude Code: paste the copied command into a terminal, then restart
   it.
3. In the app, ask: "Use OpenXER to inspect my schedule." The agent finds your
   OpenXER workspace and its projects by itself, most recently saved first, and
   asks which one you mean when it is not clear. A schedule elsewhere? Name the
   XER file or its folder.
4. Ask for the change you want, for example "List activities with no successor"
   or "Mark the foundation activities complete as of Friday and reschedule."
5. Open the file the agent wrote to see its work in OpenXER.

## What an agent can do
- Read projects, activities, relationships, calendars and resources.
- Set actual dates, remaining durations and percent complete.
- Add, change and remove relationships and their lags.
- Run the schedule or resource leveling and report the dates.
- Read the project's work products and documents (title, reference, version,
  status, file locations and the activities and WBS elements each is tied to),
  and add, change, link and delete them as the Documents view does.
- Read the text of a document's file: text, CSV, JSON, XML and log files, Word
  (.docx) files and the text of PDF files, page by page.
- Write XER, Excel and PDF files.
- See every project in your workspace with how it is doing, folder by folder (dates, float,
  schedule percent complete, finish variance, units and costs), and add, rename, move or
  delete project folders, file a project in a folder or create a new project in one.
- See the workspace's shared calendars, resources, codes and other dictionaries, and
  which projects use each one, and convert a copy of a workspace from an earlier
  version to shared dictionaries ([Shared Dictionaries](shared-dictionaries)).

## Example: how are all my jobs doing?
Ask your agent: "Use OpenXER on my workspace and tell me how all my jobs are doing."
The agent runs `openxer-cli eps list --input openxer.db` (or the `openxer_eps_list`
tool). It gets every EPS folder and project with its start, finish, data date, total float,
schedule percent complete and finish variance, and can answer, for example: "East Region
is 28% complete and finishes 05-Feb-27; BRAVO has -1 day of float and is the most
critical job." To file a job under another folder it writes a new workspace, for example
`openxer-cli eps file-project --input openxer.db --output filed.db --workspace-id 3 --folder EAST`.
The numbers follow the rules in the Projects view topic.

## Good to know
> An agent never changes the file you gave it: every edit is written to a new file.
  A batch of edits runs all or nothing.
> Ask for a dry run first to see every change before anything is written.
> An agent reads a file only through a document of the project, and only a file
  on a local drive (such as `C:\...`) that the document's location names: the
  same files **Launch...** in the [Documents](view-documents) view opens.
  Relative, network and web locations, programs and scripts are never read.
> For a connected app OpenXER itself sends nothing off this computer, but what
  your agent reads goes to the agent's own service. The one exception is
  [Chat](ai-chat) with ChatGPT or with an API key, where OpenXER sends your
  questions and the schedule data its tools read to that service.

## See also
- [Skills for AI agents](skills-for-ai-agents)
- [Scheduling basics](scheduling-basics)
- [Getting started](getting-started)
- [Documents](view-documents)
- [Projects view](view-projects)

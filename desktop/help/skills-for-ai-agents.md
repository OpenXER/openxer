# Skills for AI agents

A skill is a short set of instructions that teaches a connected AI agent one way
to analyze or build a schedule with OpenXER's tools: which tool to run, what to
take from its result, what to ask you, and how to write up the answer. OpenXER
comes with a library of skills, and you can change them or write your own.

## How an agent finds a skill
- When an agent connects, OpenXER tells it to look for a matching skill before it
  analyzes or builds a schedule, and to follow it.
- The agent lists the skills and picks the one whose "use when" matches what you
  asked. You can also name it: paste the skill's request sentence, for example
  "Run the logic health check on this schedule."
- Agents with a prompt menu also show every skill there.
- Every number an agent reports comes from an OpenXER tool. A skill never asks the
  agent to count, add up or work out a date or float itself.

## Kinds of skill
- **Check**: reads the schedule and reports. Nothing is changed.
- **Workflow**: builds or changes a schedule. It tries every change as a dry run
  first, shows you the list of changes, and writes a new file. Your file is never
  overwritten.

## The Skills dialog
Open `AI Agents > Skills...`. The list shows each skill's name, a one-line
description, its kind and where it comes from:
- **Built-in**: comes with OpenXER.
- **Yours**: a skill you wrote.
- **Customized**: your changed copy of a built-in skill. It replaces the built-in.

The buttons:
- **New**: starts a skill from a short template that explains itself.
- **Edit**: opens the skill in the editor. A built-in skill opens as a copy.
- **Duplicate**: makes a copy under a new id for you to change.
- **Delete** removes your skill. **Restore** deletes your customized copy and
  brings the built-in back.
- **Copy Request**: copies the skill's request sentence, to paste into your agent.
- **Open Folder**: opens the folder that holds your skill files.

## Writing a skill
1. Click **New**, or select a skill close to what you want and click
   **Duplicate**.
2. Fill in the name, a one-line description, the request sentence, and "use
   when": the words a person would use to ask for it.
3. Pick **Check** or **Workflow**.
4. Write the steps in the instructions box. Name the tool for each step; the
   **Tools you can name** list shows them with what each one returns.
5. Click **Save**. If something is wrong, the editor says what and where, and
   nothing is saved until it is fixed.

Good instructions say what to take from each tool's result, how to rank what
they find, what to ask when something is unclear (one question at a time), what
not to do, and the shape of the final answer. Keep them in plain words.

To write in your own text editor instead, click **Text Editor...** in the editor. The file
is plain Markdown: a header between two `---` lines, then the instructions. An
agent picks up a saved change the next time it asks for skills; no restart.

## Example: a crew peaks check
In the editor, fill in:
- **Name**: `Crew peaks`
- **Description**: `Finds the weeks each crew is over its limit and the work behind it.`
- **Request**: `Check the crew peaks.`
- **Use when**: `The user asks whether crews are overloaded or when manpower peaks.`
- **Kind**: Check

Then the instructions:
1. Run `openxer_resource_loading` by week. Take each crew's peak and the weeks
   over its limit.
2. For the worst week, run `openxer_usage` for that crew to name the activities.
3. Never add up hours yourself. Rank crews by how far over the limit they go.
4. Answer with one line per crew, then the activities behind the worst week.

Save it, then ask your agent "Check the crew peaks."

## Good to know
> A skill file that has a mistake is left out and the dialog shows why. The other
  skills keep working.
> Your skills are kept in the `skills` folder of OpenXER's data folder, next to
  the workspace.

## See also
- [Working with AI agents](working-with-ai-agents)

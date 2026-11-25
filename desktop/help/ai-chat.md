# Chat

**Chat** (`AI Agents > Chat`, or `F8`) asks an AI assistant about the open
schedule and shows its answers beside your work. You can use it as a floating
tool window or click **Dock** to pin it as a resizable right-hand pane beside
the table and Gantt chart. Use **AI Agents > Ask Chat about Selection** to
open Chat pre-loaded with a question about the selected activities.

## Choose how to connect
Chat works with one of four assistants. When none is ready yet, the window
lists them, each with what it needs and one button:
- **ChatGPT**: uses your ChatGPT plan, with nothing to install. **Sign In...**
  explains what is sent and opens the ChatGPT sign-in in your browser.
- **API key or local model**: your own API key (OpenAI, OpenRouter and other
  compatible services) or a model running on this PC (for example Ollama or
  LM Studio). **Set Up...** asks for the kind of server, its address (for
  **Other compatible server**), the API key (kept in Windows Credential
  Manager, never shown again) and the model; **Test** checks the address and
  the key without using any of your allowance. A key is kept for the address
  it was entered for and is never sent anywhere else; **Remove Key** deletes
  it. Before the first message to a service on another computer, Chat shows
  what it sends there and asks once. A model on this PC starts in
  **Ask only**: small models can answer wrongly or misuse tools.
- **Claude Code**: the Claude Code app from Anthropic, with a Claude
  subscription or API key. **Get Claude Code...** opens Anthropic's install
  page; **Check Again** looks for it once it is installed; **Sign In...** runs
  its own sign-in.
- **Codex**: the Codex app from OpenAI, with a ChatGPT plan or API key, in
  the same way.

Use of each counts against that account's plan. The first assistant that is
ready is used, and the one you choose is remembered. **Choose how to
connect...** at the end of the **Agent** list shows the list again at any
time.

## Check Setup
**Check Setup...** (in the list above, and in **Options...**) checks, one after
another and without stopping your work: whether each app is installed, its
version against the one OpenXER was tested with, whether it is signed in,
whether Codex reads a personal instructions file of yours, the ChatGPT sign-in
and plan use, whether this PC reaches the services, whether a proxy server is
set up, and whether `openxer-cli` answers. Select a row that failed and click
its fix (**Sign In...**, **Get...**, **Update...**, **Check Again**). **Copy
Results** copies the rows as text for a bug report, without accounts,
tokens or your user folder. **Stop** ends a check that takes too long.

## The window and docked pane
- **Agent**, **Options...**, **New Chat**: pick the active agent, inspect
  active rules and settings, or archive the current conversation and start a
  fresh one.
- **Model**, **History**, **Dock / Float**: switch models directly without
  opening Options, jump back to earlier conversations for the open project
  from **History**, or toggle between a docked right-hand pane (with a
  draggable splitter) and a floating window.
- **Hiding without losing work**: pressing `F8`, `Escape`, or the close box
  hides Chat and stops its background agent process cleanly while keeping your
  conversation visible when you reopen it. Sending another message continues
  the thread automatically.
- **The conversation**:
  - Activity IDs in replies, tables, tool rows and proposed changes are links:
    hover an ID to preview its name, dates and total float, or click it to
    select that activity in the table.
  - Click and drag across any user message, reply, heading, list or note to
    select text, then press `Ctrl+C` or right-click **Copy Selection**.
    Beneath each finished reply, click **Copy** (or right-click for plain text
    or Markdown) and **Select all (N)** when multiple activities are mentioned.
    Rendered tables include **Copy table for Excel** (tab-separated cells).
  - Multi-tool runs fold into a single expandable summary row once finished so
    answers stay easy to scan; failed or running tools stay expanded.
  - When a request is ambiguous or has options, the assistant can show an
    interactive question card with clickable choices and a box to add your own
    context before clicking **Send Answer** (or **Skip**).
- **Context and Prompts**: check **Context** to send the project, selected
  activities, data date, assigned baseline, active filter and open Details tab
  with each turn (hover the field to inspect what is sent). Click **Prompts**
  to run built-in or custom playbooks, manage saved prompt snippets (supporting
  `{selection}`, `{project}` and `{data_date}` tokens), or open **Project Chat
  Rules...**.
- **The message box**: `Enter` sends, `Shift+Enter` starts a new line, and the
  box grows automatically for multi-line prompts. Type `/` to filter and run
  playbooks or saved prompts (such as `/check`, `/audit`, `/lookahead`,
  `/narrative`, `/critical`, `/recover`, `/earned-value`, `/resource-review`,
  `/what-if`, `/update`, `/build`), type `@` to autocomplete Activity IDs, and
  press the Up or Down arrow key when empty to recall earlier prompts. While an
  answer is being written **Send** becomes **Stop**, and you can keep typing
  and press `Enter` (or click **Queue**) to queue follow-up messages above the
  context bar, or click **Send now** (or press Ctrl+Enter) to interrupt and
  send immediately.
- **Mode**: **Ask only** answers questions; **Edit** lets the agent propose
  changes. If an answer in **Ask only** mode suggests switching to Edit, click
  **Switch to Edit & Retry**.
- **Tokens**: after an answer, the tokens used show beside **Send**; hover for
  both last-reply and cumulative session totals (**tokens not reported** when
  the server does not count them).
- With ChatGPT, **Using ChatGPT plan** shows above the context line, with
  **Manage usage** (your usage settings in ChatGPT).

## Playbooks, custom skills and standing rules
Chat uses a three-layer instruction stack: OpenXER's built-in scheduling core,
your global standing instructions (`chat\AGENTS.md`, opened via
**Options... > Instructions...**, up to 4,000 characters), and optional
**Project Chat Rules...** saved per project (up to 2,000 characters).

Built-in scheduling playbooks are loaded on demand via `openxer_playbook` and
can be launched directly from `/` or the **Prompts** menu. Drop any `.md` file
into `chat\playbooks\` (**Options... > Playbooks...**) to add your own custom
playbooks and `/commands` automatically.

## Controlling OpenXER from Chat
You can ask the assistant to drive the open OpenXER window directly (in both
**Ask only** and **Edit** mode): apply, highlight, save or clear filters
(standard filters, Check Schedule filters, saved filters, Activity ID lists or
custom multi-rule filters), change Group and Sort, add/remove/reorder/reset
columns, select or jump to activities, trace backward/forward Activity Critical
Paths, switch main/top/bottom views and Activity Details tabs, zoom or toggle
relationships on the Gantt chart, open any dialog, export files (Excel, S-curve,
Earned Hours, PDF, XER, XML), or undo and redo.

## Proposed changes
In **Edit** mode the agent proposes changes instead of making them. The
proposal card shows a schedule impact summary (finish date movement, driving
path changes and edited counts) plus each change row (Activity ID, field, old
and new value). Large proposals fold after 8 rows with **Show all N changes**,
and clicking an Activity ID row jumps straight to that activity before you
approve.

Nothing is written until you click **Apply**. **Apply** is one step: `Ctrl+Z`
or the inline **Undo** link beside **Applied.** undoes it. Once applied, Chat
automatically notifies the assistant of the updated schedule so it can continue
any remaining steps without you having to prompt it again. If you changed the
project after the proposal was made, **Apply** first shows the changes as they
would be now; click **Apply** again if they are still right. **Discard** throws
the proposal away. `Enter` in the message box still sends a message.

## Rolling the window up
When floating, the arrow beside the close box (or a double-click on the title
bar) rolls the window up to its title bar, which then says what is happening,
for example "Chat - 1 change ready" or "Chat - answering...". Do the same to
roll it down. Drag the title bar to move the window and the corner grip to
resize it. Its place, size, docked state, agent, model and mode are kept for
the next time.

## Good to know
> Claude Code and Codex run on this PC under their own sign-in: OpenXER itself
  sends nothing off this computer, and what they read goes to their own
  service. With ChatGPT, OpenXER itself sends your questions and the schedule
  data the assistant reads to OpenAI, under your ChatGPT account; it asks once
  before the first sign-in. With an API key, OpenXER sends them to that
  service and asks once per service before the first message; a model on this
  PC keeps them here.
> The assistant reads a copy of the open project in a temporary folder of
  its own, never the project itself. The copy is removed whenever the
  conversation ends or Chat is hidden (**New Chat**, closing or hiding Chat,
  closing the project, or closing OpenXER).
> When an agent reaches its usage limit the conversation says so and nothing is
  sent again until you send a new message. With ChatGPT, **Manage usage** opens
  your usage settings.
> An app older than the version OpenXER was tested with gets a note and is
  still tried; one that refuses how OpenXER starts it says it is not
  supported: update it.
> Every assistant reads the schedule through OpenXER's own tools. If Claude
  Code or Codex cannot reach them, its answer is not shown: the conversation
  says so, and you can try again.

## See also
- [Working with AI agents](working-with-ai-agents)
- [Keyboard shortcuts](keyboard-shortcuts)

# AGENTS.md

Instructions for AI coding agents (and the humans who direct them) working on
OpenXER. Read the whole file before changing anything. If an `AGENTS.local.md`
exists next to this file, read it too: it holds the maintainer's machine
setup and is never committed.

## What OpenXER is

OpenXER is a desktop CPM scheduler for XER files, written in Rust. It opens,
schedules, levels, edits and saves XER schedules, and every edit is also
available to AI agents through `openxer-cli` and its MCP server.

| Crate | What it is | Depends on |
|---|---|---|
| `engine` | CPM scheduling, calendars, resource leveling | nothing |
| `xer` | XER read/write and the shared edit layer | engine |
| `interchange` | XER export and merge, baselines, Excel (XLSX) import and export, export templates, report and S-curve workbooks | engine, xer |
| `desktop` | the app (egui/eframe) | engine, xer, interchange, automation (the AI Agents menu uses `connect`) |
| `automation` | `openxer-cli` and the MCP server | engine, xer, interchange |
| `chat` | `openxer-chat`: Sign in with ChatGPT, tokens in Windows Credential Manager, and a streaming Responses API client with the tool loop, for the Chat window; `compat`: an OpenAI-compatible endpoint with the user's API key or a model on this PC (kept out of `openxer-cli`; its live examples are `cargo run -p openxer-chat --example live_chat` and `--example compat_chat`) | nothing (engine and xer for its examples only) |

Also tracked: `README.md`, `automation/README.md` (CLI and MCP reference),
`automation/COVERAGE.md` (which app command or field each CLI command and MCP
tool covers, and the gaps),
`THIRD_PARTY.md` (license notices), `assets/` (icons and starter report
definitions compiled into the app, `assets/sample-project.txt`, the
invented data table the first-run sample project is built from by
`xer/src/sample_project.rs`, `assets/playbooks/`, the scheduling
playbooks the chat tool `openxer_playbook` serves, and `assets/skills/`,
the built-in skills for AI agents compiled into `openxer-cli` and the app), `desktop/assets/spell/` (word lists),
`tools/icons/` (the icon generator), `tools/spell/` (the word list generator
`THIRD_PARTY.md` cites), `tools/licenses/third_party.py` (regenerates the
crate and SQLite notices in `THIRD_PARTY.md` for the Windows and Linux
executables together, the same output on either system; run it after a
dependency change), `tools/winres.rs` (the Windows icon, version and manifest resources
the `desktop` and `automation` build scripts embed), `docs/images/` (README screenshots, rendered from a
synthetic sample schedule by the ignored tests in
`desktop/src/readme_hero.rs`: `cargo test -p openxer-desktop --bin OpenXER readme -- --ignored`).
Screenshots in the repo are only ever rendered from that synthetic sample.

For any UI work read `DESIGN.md` first: the one shared implementation of each
control, the standard measurements and the tests that guard them.

For engine work read `engine/RULES.md` (each scheduling rule, with an ID that
code and tests cite, and the evidence for it) and `engine/PARITY.md` (how the
engine compares with the results stored in real schedule files).

## Accuracy is the product

People trust a scheduler only if its dates are right. Every engine rule is
backed by evidence: a real XER whose stored results it reproduces, or a
controlled test case. Never change scheduling behavior without that evidence,
a test that pins it, and the regression counts below staying exact.

## Building and running

- Rust stable with the MSVC target on Windows. `cargo build -p openxer-desktop`
  builds the app; `cargo build -p openxer-automation` builds `openxer-cli`.
- Linux (CI runs Ubuntu 22.04): the same commands, with no extra flags or
  features. Building needs only a C compiler (`build-essential`, for the
  bundled SQLite); the X11, Wayland, OpenGL and file-dialog portal
  libraries are loaded at run time, which a desktop session already has.
  Install `fonts-liberation` so text measures close to Windows: the app
  uses the real Arial, Segoe UI and Tahoma when installed (or under
  `WINDIR`, such as `/mnt/c/Windows` in WSL), else Liberation Sans, else
  DejaVu Sans. Run the tests as a normal user, not root.
- Release: `cargo build --release -p openxer-desktop -p openxer-automation`, or
  `python desktop/build_release.py --output <folder>\OpenXER.exe` for the
  packaged static-runtime build with `CLI-MCP.md`, the license files,
  `THIRD_PARTY.md` and an `openxer.mcp.json` template (its command is a
  placeholder, never a path from the build machine). `--package-only`
  writes just those files, without building. `--installer` also builds
  `target/installer/OpenXER-<version>-setup.exe` from those files with Inno
  Setup 6 (`tools/installer/openxer.iss`; the version comes from Cargo);
  `--sign-command "<cmd with $f>"` signs the executables and Setup.
- Workspace location (`Store::default_path_in`): `OPENXER_DATA_DIR` when
  set; an installed copy (the `openxer.installed` marker Setup puts beside
  the EXE) uses `%LOCALAPPDATA%\OpenXER`; a portable copy uses `openxer.db`
  beside the EXE, or `%LOCALAPPDATA%\OpenXER` when that folder is not
  writable. On other systems `$XDG_DATA_HOME/OpenXER` (else
  `~/.local/share/OpenXER`) takes the place of `%LOCALAPPDATA%\OpenXER`
  (`openxer_automation::locate::user_data_base`). Never write a test that
  touches the real data folder: set `OPENXER_DATA_DIR` or pass `--database`.
- The app holds the mutex `OpenXER.App` while it runs; Setup's `AppMutex`
  names it. Keep the two in step.
- Line endings are LF (`.gitattributes`). From Python on Windows write files
  with `newline=''` or `'\n'`.
- Machine-specific linker paths go in `.cargo/config.toml`, which is ignored.

## Tests and the gate

```
cargo fmt --all --check
cargo clippy -p <crates> --all-targets -- -D warnings
cargo test -p <crates> --no-fail-fast -- --skip full_ui_sweep --skip dialog_formatting_sweep
# if you changed menus, dialogs, toolbars, shortcuts or the dialog kit:
cargo test -p openxer-desktop -- full_ui_sweep dialog_formatting_sweep
```

Test the crates you changed and every crate that depends on them (table
above). Clippy must be clean with `-D warnings`.

Some tests compare against real schedules that hold client data. Those inputs
never live in the repo. They are found through environment variables (set
them in the ignored `.cargo/config.toml` under `[env]`), and the tests skip
with a printed reason when the variable is not set:

| Variable | What it points at |
|---|---|
| `OPENXER_FIXTURES` | engine regression fixtures (`oxdiff` and engine tests) |
| `OPENXER_CORPUS` | a folder of corpus XER files |
| `OPENXER_RPT_INPUTS`, `OPENXER_RPT_DB` | report definition import tests |

Lists that must not be published are private fixtures in the
`OPENXER_FIXTURES` folder too: `corpus-names.txt` (corpus files whose names
identify a job, read through `openxer_engine::fixture::corpus_names`) and
`internal-refs.txt` (words the engine sources must not contain, read by
`engine/tests/no_internal_refs.rs`). Tests that need them skip with a
printed reason when they are absent.

When the fixtures are available, any change under `engine/` or to how `xer`
derives durations must keep `oxdiff` exact
(`cargo build --release -p openxer-engine --bin oxdiff`, then with
`OPENXER_FIXTURES` set in the shell run `oxdiff cal`, `oxdiff forward`,
`oxdiff engine`, `oxdiff roundtrip`, `oxdiff engine regressions` and
`oxdiff engine features`): cal 77620, forward 19624, engine 86356,
roundtrip 17, regressions 129622, features 424. A different count fails.

Useful desktop test filters: `headless_ui`, `headless_render` (renders go to
the ignored `artifacts/`), `qa_sweep`, and the ignored `perf_369`.

## Rules that are never bent

- **No client data in the repo, ever.** No XER files from real jobs,
  workspace databases, screenshots or renders of real projects, or fixtures.
  Test data that is committed must be synthetic. Issues and pull requests must
  not attach real schedules either.
- **Original work only.** OpenXER may follow the functional conventions of
  scheduling software users already know (layouts, control types, short
  labels, menu and dialog structure). It never copies protected expression:
  - no other product's name or logo in UI text, CLI or MCP output, docs or
    packaging, beyond plain compatibility statements ("reads and writes XER
    files");
  - no icons, bitmaps or artwork from other products, even recolored or
    traced (icons are drawn for this project: `chrome::Icon`, `tools/icons/`);
  - no code, help text, documentation prose, reports or data files from other
    products, and never decompiled or disassembled material of any kind;
  - our own names in code (`activity`, `link`, `calendar`), except where a
    file format defines the name (XER table and field names stay as the
    format spells them).
- **Default colors are OpenXER's own** (`desktop/src/theme.rs`). A color the
  user chose is stored as chosen; defaults only fill in where nothing was
  stored. Layouts are user-level, not per project.
- **Headless only.** Never open native windows or move the system cursor.
  Drive and look at the UI through the test harnesses
  (`desktop/src/headless_ui.rs`, `headless_render.rs`, the `qa_sweep.rs`
  driver). Do not launch `OpenXER.exe` unless the maintainer asks.
- **No em dashes or en dashes** anywhere (code, comments, docs, UI text,
  commit messages). Use a hyphen or rewrite the sentence.
- **Never stop or kill processes you did not start.**
- **Keep the repo lean.** Track only what building, running, testing and
  developing OpenXER need. No plans, run logs, status files, prompts, research
  notes or sample outputs.

## How the app is built (conventions)

- Every command that changes project data is one undo step and refreshes
  caches (`edit_step` and `records_revision`).
- Every dialog closes on Escape; Cancel or Close leaves the project unchanged.
  The QA sweep clicks every menu command and checks this.
- Data an XER carries must import, survive save and reload, and export.
  Unknown tables pass through untouched.
- Edits from anywhere (table, Details, import, CLI, MCP) go through the shared
  edit layer in `xer/src/edit/` and the desktop setters that delegate to it,
  so the app, the CLI and MCP always behave the same.
- Every feature that changes project data gets a CLI command and an MCP tool
  from the same command definition in `automation/`.
- A skill (`assets/skills/*.md`, `automation/src/skills.rs`) names only tools
  that exist, and the agent never computes schedule numbers itself: a number
  a skill needs comes from a read-only tool (add one first) or the step goes.
  Built-in skills say nothing about where a method comes from.
- A control that already exists is reused, or extracted into a shared
  function, never written a second time (the details tab strip is
  `desktop/src/tab_strip.rs`). Where two views differ, the Activities page
  is the reference.

## Working on a change

- One branch per change, off `main`, in its own worktree if several agents
  work at once. Do not commit to `main` directly; open a pull request.
- Describe what you built and what is unverified in the commit message and the
  pull request, with the gate results.

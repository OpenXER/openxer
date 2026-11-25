# Contributing to OpenXER

Thank you for helping. OpenXER is a scheduler, so the bar is accuracy: a
change that moves a date needs evidence and a test.

## Before you start

- Open an issue first for anything larger than a small fix, so the approach
  can be agreed before you write it.
- If you use an AI coding agent, point it at [AGENTS.md](AGENTS.md). The rules
  there apply to human contributors too.

## Build and test

```text
cargo build -p openxer-desktop -p openxer-automation
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --skip full_ui_sweep --skip dialog_formatting_sweep
```

Run `cargo test -p openxer-desktop -- full_ui_sweep dialog_formatting_sweep`
as well when you change menus, dialogs, toolbars or shortcuts.

Some tests compare against real schedules that only the maintainers hold.
They skip with a printed reason on your machine; that is expected. The
maintainers run them before merging.

## What a good pull request has

- One focused change, with a clear description of what it does and why.
- Tests for new behavior. Scheduling changes need a test that pins the exact
  dates or float, and a short note of the evidence (for example: "an XER saved
  by a scheduler with these options shows these dates").
- `cargo fmt`, clippy with `-D warnings` and the tests passing.
- No unrelated reformatting or renames.

## AI-assisted pull requests

AI coding agents are welcome here (read [AGENTS.md](AGENTS.md) first). Every
pull request still needs a human author who ran the checks, verified the
behavior and can explain the change.

## Contributing scheduling skills

You do not need to write Rust to contribute. OpenXER's built-in skills in
`assets/skills/` are plain Markdown files that guide connected AI agents
through real scheduling reviews and workflows. Open a pull request to improve
an existing skill or propose a new one; keep every example invented and free
of real project data.

## Hard rules

- **No client or project data.** Never commit or attach real schedules,
  workspace databases, or screenshots of real projects, in code, tests,
  issues or pull requests. Build test data in code or use invented examples.
- **Original work only.** Do not contribute code, icons, artwork, help text or
  data taken from other software, and never anything decompiled or
  disassembled. Do not use other products' names or logos in the app or docs.
- **No em dashes or en dashes** in code, comments, docs or UI text.

## Reporting bugs

Use the bug report form. Describe the schedule shape that triggers the
problem (activity types, relationship types, calendars, options) rather than
attaching a real file. If a file is needed to reproduce it, make a small
invented one.

Security problems: see [SECURITY.md](SECURITY.md); do not open a public issue.

## License

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in OpenXER by you, as defined in the Apache-2.0
license, shall be dual licensed as MIT OR Apache-2.0, without any additional
terms or conditions.

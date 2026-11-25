# Roadmap

OpenXER is developed in the open. Contributions, forks and pull requests are
welcome; open an issue or discussion before starting larger work so we can
align on the design.

## Shipped in v0.1.0

- Native desktop CPM scheduler for Windows and Linux (`OpenXER` and `openxer-cli`)
- Full XER round-trip preservation, project XML import/export, Excel (XLSX)
  import/export, S-curve and Earned Hours workbooks, and vector PDF printing
- Critical path scheduling, calendars, constraints, Level of Effort, progress
  updates, baselines, schedule comparison, Check Schedule, resource usage and
  resource leveling
- Built-in AI Chat (`F8`) with live tool calls, clickable Activity IDs,
  reviewable edit proposals, and Bring Your Own AI (ChatGPT sign-in, Claude
  Code, Codex, OpenAI-compatible API keys, or local models)
- Built-in MCP server (`openxer-cli mcp`) and 30 prebuilt Markdown scheduling
  skills (`AI Agents > Skills`)

## Next up

- **macOS support:** native macOS builds, packaging and CI coverage
- **Code signing:** signed Windows installers (SignPath / Authenticode) so
  first-run SmartScreen prompts go away
- **Linux package channels:** Flatpak / distribution packages alongside the
  AppImage and portable tarball
- **Expanded synthetic benchmark suite:** larger public invented schedules for
  external contributors testing engine, leveling and baseline PRs
- **Community scheduling skills:** more domain playbooks and skills under
  `assets/skills/` (turnarounds, megaprojects, forensic delay, cost loading)
- **Additional CLI and MCP parity:** closing the remaining read and edit gaps
  tracked in [automation/COVERAGE.md](automation/COVERAGE.md)

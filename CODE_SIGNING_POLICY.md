# Code Signing Policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

## Scope

This policy covers the official Windows binaries published on the [GitHub Releases](https://github.com/OpenXER/openxer/releases) page of `https://github.com/OpenXER/openxer`:

- `OpenXER.exe` (desktop application)
- `openxer-cli.exe` (command-line interface and MCP server)
- `OpenXER-<version>-setup.exe` (Windows installer)

All signed binaries are built exclusively from this public repository on GitHub-hosted runners (`windows-latest`) through [.github/workflows/release.yml](.github/workflows/release.yml). Third-party dependencies keep their own licenses and notices as documented in [THIRD_PARTY.md](THIRD_PARTY.md).

## Team roles

- **Approvers:** `@clachance14` (repository owner; approves signing requests for tagged releases, with two-factor authentication enabled on GitHub)
- **Reviewers:** `@clachance14` (reviews pull requests from external contributors before merge into `main`)
- **Committers:** `@clachance14` (maintainers with write access to `OpenXER/openxer`)

## Privacy policy

OpenXER runs locally and offline on the user's computer. This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it:

- OpenXER includes no telemetry, analytics, crash uploaders or license checks.
- Schedule files and workspace databases stay on the user's local disk.
- Network requests occur only when the user explicitly enables the built-in AI Chat feature with Sign in with ChatGPT or a user-configured external API endpoint, in which case chat prompts and the schedule data requested by its tools are sent only to that chosen provider. Using local models or offline scheduling sends no data off the computer.

## Reporting security issues

Please report security issues privately following [SECURITY.md](SECURITY.md).

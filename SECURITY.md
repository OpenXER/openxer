# Security policy

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub: open the repository's **Security** tab
and choose **Report a vulnerability**. Include what you found, how to
reproduce it, and which version you used. Do not include real project
schedules; describe or invent the input that triggers the problem.

You will get an acknowledgement, and a fix or a plan, as soon as the
maintainers have reproduced the problem.

## Scope

OpenXER runs locally. Of most interest:

- crafted XER, Excel or workspace files that crash the app, corrupt a
  workspace, or run code;
- the CLI or MCP server writing outside the output path it was given, or
  modifying an input file;
- anything that sends schedule data off the machine.

## Supported versions

Fixes go into the latest release.

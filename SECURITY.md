# Security Policy

This repository is an independent fork of
[v8/vscode-torque](https://github.com/v8/vscode-torque). It is not affiliated
with Google or the V8 project.

## Supported versions

Security updates are issued for the latest published release of this fork
(`riya-amemiya.vscode-torque-fork`).

| Version | Supported |
| ------- | --------- |
| 1.2.x   | Yes       |
| < 1.2   | No        |

## Reporting a vulnerability

Do **not** open a public GitHub issue for security reports.

Prefer GitHub private vulnerability reporting:

https://github.com/riya-amemiya/vscode-torque-fork/security/advisories/new

If that form is unavailable, email
[riya-amemiya+github@tokidux.com](mailto:riya-amemiya+github@tokidux.com)
with:

- A description of the issue and its impact
- Affected extension versions, editor versions, and operating systems
- Steps to reproduce

Please do not include secrets, tokens, or access to systems you do not own.

### What to expect

- We aim to acknowledge reports within 7 days.
- We will follow up as we investigate.
- Accepted reports are fixed in a release and disclosed through a GitHub
  Security Advisory when a patched version is available.
- If a report is declined, we will explain why.

## Scope

In scope:

- This VS Code / Cursor extension and its published `.vsix`
- GitHub Actions workflows and the release process in this repository

Out of scope:

- The V8 Torque compiler, `torque-language-server`, and other V8 components
- The original [v8/vscode-torque](https://github.com/v8/vscode-torque) project
- Language questions about Torque itself ([Torque user manual](https://v8.dev/docs/torque))

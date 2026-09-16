# Contributing

This repository is an independent fork of
[v8/vscode-torque](https://github.com/v8/vscode-torque).
It is not affiliated with Google or the V8 project.

Please do **not** sign or submit a [Google Contributor License
Agreement](https://cla.developers.google.com/) for this repository. That CLA
applies to Google-hosted projects, not to this fork.

## How to contribute

1. Open an issue before starting a large change.
2. Fork the repository and open a pull request against `main`.
3. Keep the pull request focused on one change.

## Development

Install dependencies with [Bun](https://bun.sh/), then run the same checks CI
runs on every pull request:

```
bun install
bun run lint
bun run format:check
bun run typecheck
bun run compile
bun test src
```

`bun run format` rewrites files to the repository style.

## License of contributions

This project is licensed under the Apache License, Version 2.0. By opening a
pull request, you agree that your contribution is licensed under the same
Apache License, Version 2.0, and that you have the right to submit it.

Keep existing copyright and license headers. If you change a file that already
has a header, leave the original notice in place and add a short note that the
file was modified.

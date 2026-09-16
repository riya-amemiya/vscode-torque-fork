# V8 Torque Language Support Fork

This extension adds language support for the V8 project internal language
[V8 Torque](https://v8.dev/docs/torque).

This repository is an independent fork of
[v8/vscode-torque](https://github.com/v8/vscode-torque).
Some files have been modified so the extension works with current
development environments. It is not affiliated with, endorsed by, or
supported by Google LLC or the V8 project.

The Marketplace identifier is `riya-amemiya.vscode-torque-fork`.

## Language server

The language server is not built by default. To build the language server manually:

```
cd <v8 checkout>
autoninja -C <output dir> torque-language-server
```

The default directory where the extension looks for the executable is `out/x64.release`,
but the absolute path to the executable can be configured with the `torque.ls.executable`
setting.

## Installation instructions

Download the `.vsix` from
[GitHub Releases](https://github.com/riya-amemiya/vscode-torque-fork/releases).
In Visual Studio Code, open the Extensions view, choose **Install from VSIX...**,
and select the downloaded file. Open any `.tq` file to activate the extension.

A `v*` git tag starts the release workflow, which attaches that `.vsix` to the
GitHub Release. If the `VSCE_PAT` repository secret is set, the same workflow
also publishes to the Visual Studio Marketplace.

## License

This project is a derivative work of
[v8/vscode-torque](https://github.com/v8/vscode-torque),
Copyright 2019 The VSCode V8 Torque Authors, licensed under the
Apache License, Version 2.0.

Modifications in this repository are Copyright 2026 Riya Amemiya
and are also licensed under the Apache License, Version 2.0.

See [LICENSE](LICENSE) for the full license text and [NOTICE](NOTICE)
for attribution. See [CONTRIBUTING.md](CONTRIBUTING.md) if you want
to send a change to this fork.

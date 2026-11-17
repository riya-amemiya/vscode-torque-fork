// Copyright 2019 The VSCode V8 Torque Authors.
// Copyright 2026 Riya Amemiya.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// This file has been modified from the original vscode-torque sources.

// The file dist/extension.js is generated from src/extension.ts and
// should not be modified manually.

import { window, workspace, type ExtensionContext } from "vscode";
import { TorqueWorkspace } from "./language/workspace";
import {
  createTorqueStatusBar,
  stopNativeLanguageServer,
  tryStartNativeLanguageServer,
} from "./native-ls";
import { registerTorqueLanguage } from "./torque-language";

export async function activate(context: ExtensionContext) {
  const output = window.createOutputChannel("Torque Language Server", { log: true });
  const statusBarItem = createTorqueStatusBar();
  const store = new TorqueWorkspace();
  context.subscriptions.push(output, statusBarItem);
  const setNativeRunning = registerTorqueLanguage(context, store);
  if (workspace.isTrusted) {
    await tryStartNativeLanguageServer(context, output, statusBarItem, setNativeRunning);
    return;
  }
  context.subscriptions.push(
    workspace.onDidGrantWorkspaceTrust(() => {
      void tryStartNativeLanguageServer(context, output, statusBarItem, setNativeRunning);
    }),
  );
}

export function deactivate(): Thenable<void> | undefined {
  return stopNativeLanguageServer();
}

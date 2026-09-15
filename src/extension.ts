// Copyright 2019 The VSCode V8 Torque Authors.
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

// The file out/extension.js gets automatically created from
// src/extension.ts. out/extension.js should not be modified manually.

import * as path from "node:path";
import {
  StatusBarAlignment,
  window,
  workspace,
  type ExtensionContext,
  type LogOutputChannel,
  type WorkspaceConfiguration,
} from "vscode";
import {
  CloseAction,
  ErrorAction,
  LanguageClient,
  RevealOutputChannelOn,
  State,
  type CloseHandlerResult,
  type ErrorHandler,
  type ErrorHandlerResult,
  type LanguageClientOptions,
  type Message,
  type ServerOptions,
} from "vscode-languageclient/node";

const clientRef: { value: LanguageClient | undefined } = { value: undefined };

class TorqueErrorHandler implements ErrorHandler {
  constructor(private readonly output: LogOutputChannel) {}

  error(
    error: Error,
    message: Message | undefined,
    _count: number | undefined,
  ): ErrorHandlerResult {
    this.output.error(error.toString());
    if (message !== undefined) {
      this.output.error(message.toString());
    }
    return { action: ErrorAction.Continue };
  }

  closed(): CloseHandlerResult {
    return { action: CloseAction.DoNotRestart };
  }
}

function resolveServerExecutable(config: WorkspaceConfiguration): string {
  const configured = config.get<string | null>("executable");
  if (typeof configured === "string" && configured.length > 0) {
    return configured;
  }
  const workspacePath = workspace.workspaceFolders?.[0]?.uri.fsPath ?? "";
  return path.join(workspacePath, "out", "x64.release", "torque-language-server");
}

function resolveServerArguments(config: WorkspaceConfiguration): string[] {
  if (config.get<boolean>("logging") !== true) {
    return [];
  }
  const logfile = config.get<string>("logfile");
  if (logfile === undefined || logfile === "") {
    return [];
  }
  return ["-l", logfile];
}

export async function activate(context: ExtensionContext) {
  const statusBarItem = window.createStatusBarItem("torque.ls.status", StatusBarAlignment.Left, 0);
  statusBarItem.name = "Torque Language Server";
  statusBarItem.text = "torque-ls: <unknown>";
  statusBarItem.show();

  const torqueConfiguration = workspace.getConfiguration("torque.ls");
  const serverOptions: ServerOptions = {
    command: resolveServerExecutable(torqueConfiguration),
    args: resolveServerArguments(torqueConfiguration),
  };

  const outputChannel = window.createOutputChannel("Torque Language Server", {
    log: true,
  });

  const clientOptions: LanguageClientOptions = {
    diagnosticCollectionName: "torque",
    documentSelector: [{ scheme: "file", language: "torque" }],
    errorHandler: new TorqueErrorHandler(outputChannel),
    initializationFailedHandler: (error) => {
      outputChannel.error(String(error));
      return false;
    },
    outputChannel,
    revealOutputChannelOn: RevealOutputChannelOn.Info,
  };

  const client = new LanguageClient(
    "torque",
    "Torque Language Server",
    serverOptions,
    clientOptions,
  );
  clientRef.value = client;

  context.subscriptions.push(statusBarItem, outputChannel);

  client.onDidChangeState((event) => {
    if (event.newState === State.Running) {
      statusBarItem.text = "torque-ls: Running";
    } else if (event.newState === State.Starting) {
      statusBarItem.text = "torque-ls: Starting";
    } else if (event.newState === State.StartFailed) {
      statusBarItem.text = "torque-ls: Start failed";
    } else {
      statusBarItem.text = "torque-ls: Stopped";
    }
  });

  await client.start();

  const urls = await workspace.findFiles("**/*.tq");
  await client.sendNotification("torque/fileList", {
    files: urls.map((url) => url.toString()),
  });
}

export function deactivate(): Thenable<void> | undefined {
  return clientRef.value?.stop();
}

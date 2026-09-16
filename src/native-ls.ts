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

import { access } from "node:fs/promises";
import * as path from "node:path";
import {
  StatusBarAlignment,
  window,
  workspace,
  type ExtensionContext,
  type LogOutputChannel,
  type StatusBarItem,
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

export function resolveServerExecutable(
  config: WorkspaceConfiguration,
  workspacePath: string,
): string {
  const configured = config.get<string | null>("executable");
  if (typeof configured === "string" && configured.length > 0) {
    return configured;
  }
  return path.join(workspacePath, "out", "x64.release", "torque-language-server");
}

export function resolveServerArguments(config: WorkspaceConfiguration): string[] {
  if (config.get<boolean>("logging") !== true) {
    return [];
  }
  const logfile = config.get<string>("logfile");
  if (logfile === undefined || logfile === "") {
    return [];
  }
  return ["-l", logfile];
}

async function executableExists(file: string): Promise<boolean> {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}

export async function tryStartNativeLanguageServer(
  _context: ExtensionContext,
  output: LogOutputChannel,
  statusBarItem: StatusBarItem,
): Promise<void> {
  const torqueConfiguration = workspace.getConfiguration("torque.ls");
  const workspacePath = workspace.workspaceFolders?.[0]?.uri.fsPath ?? "";
  const command = resolveServerExecutable(torqueConfiguration, workspacePath);
  if (!(await executableExists(command))) {
    output.info(
      `Native torque-language-server not found at ${command}; using the built-in Torque WASM compiler.`,
    );
    statusBarItem.text = "torque: wasm";
    return;
  }

  const serverOptions: ServerOptions = {
    command,
    args: resolveServerArguments(torqueConfiguration),
  };
  const clientOptions: LanguageClientOptions = {
    diagnosticCollectionName: "torque",
    documentSelector: [{ scheme: "file", language: "torque" }],
    errorHandler: new TorqueErrorHandler(output),
    initializationFailedHandler: (error) => {
      output.error(String(error));
      return false;
    },
    outputChannel: output,
    revealOutputChannelOn: RevealOutputChannelOn.Never,
  };
  const client = new LanguageClient(
    "torque",
    "Torque Language Server",
    serverOptions,
    clientOptions,
  );
  clientRef.value = client;
  client.onDidChangeState((event) => {
    switch (event.newState) {
      case State.Running:
        statusBarItem.text = "torque-ls: Running";
        break;
      case State.Starting:
        statusBarItem.text = "torque-ls: Starting";
        break;
      case State.StartFailed:
      case State.Stopped:
        statusBarItem.text = "torque: wasm";
        break;
      default: {
        const exhaustive: never = event.newState;
        return exhaustive;
      }
    }
  });

  try {
    await client.start();
  } catch (error) {
    output.error(`Failed to start torque-language-server: ${String(error)}`);
    statusBarItem.text = "torque: wasm";
    clientRef.value = undefined;
    return;
  }

  const urls = await workspace.findFiles("**/*.tq");
  await client.sendNotification("torque/fileList", {
    files: urls.map((url) => url.toString()),
  });
}

export function stopNativeLanguageServer(): Thenable<void> | undefined {
  return clientRef.value?.stop();
}

export function createTorqueStatusBar(): StatusBarItem {
  const statusBarItem = window.createStatusBarItem("torque.ls.status", StatusBarAlignment.Left, 0);
  statusBarItem.name = "Torque Language";
  statusBarItem.text = "torque: wasm";
  statusBarItem.show();
  return statusBarItem;
}

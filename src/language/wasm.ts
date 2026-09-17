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

import { existsSync, readFileSync } from "node:fs";
import * as path from "node:path";

export type CompilerDiagnostic = {
  message: string;
  start: number;
  end: number;
  severity: "error";
};

export type CompilerSymbol = {
  name: string;
  kind: string;
  start: number;
  end: number;
  containerName?: string;
  detail?: string;
};

export type CompilerInclude = {
  path: string;
  start: number;
  end: number;
};

export type CompilerDefinition = {
  fromStart: number;
  fromEnd: number;
  toUri: string;
  toStart: number;
  toEnd: number;
};

export type CompilerFile = {
  uri: string;
  diagnostics: CompilerDiagnostic[];
  symbols: CompilerSymbol[];
  includes: CompilerInclude[];
  definitions: CompilerDefinition[];
};

type WasmExports = {
  memory: WebAssembly.Memory;
  torque_alloc: (size: number) => number;
  torque_free: (ptr: number, size: number) => void;
  torque_compile: (ptr: number, len: number) => number;
  torque_result_len: () => number;
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();

let wasmExports: WasmExports | undefined;

function wasmCandidates(): string[] {
  const here = typeof __dirname === "string" ? __dirname : process.cwd();
  return [
    path.join(here, "torque_compiler.wasm"),
    path.join(here, "..", "torque_compiler.wasm"),
    path.join(here, "..", "..", "dist", "torque_compiler.wasm"),
    path.join(
      here,
      "..",
      "..",
      "target",
      "wasm32-unknown-unknown",
      "release",
      "torque_compiler.wasm",
    ),
    path.join(process.cwd(), "dist", "torque_compiler.wasm"),
    path.join(process.cwd(), "target", "wasm32-unknown-unknown", "release", "torque_compiler.wasm"),
  ];
}

export function loadTorqueCompiler(wasmBytes: Uint8Array): void {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(wasmBytes), {});
  wasmExports = instance.exports as unknown as WasmExports;
}

export function resetTorqueCompiler(): void {
  wasmExports = undefined;
}

export function ensureTorqueCompiler(): void {
  if (wasmExports !== undefined) {
    return;
  }
  const wasmPath = wasmCandidates().find((candidate) => existsSync(candidate));
  if (wasmPath === undefined) {
    throw new Error("torque_compiler.wasm not found; run `bun run build:wasm`");
  }
  loadTorqueCompiler(readFileSync(wasmPath));
}

export function compileSources(files: Array<{ uri: string; text: string }>): CompilerFile[] {
  ensureTorqueCompiler();
  const wasm = wasmExports;
  if (wasm === undefined) {
    throw new Error("Torque WASM compiler is not loaded");
  }
  try {
    const payload = encoder.encode(JSON.stringify({ files }));
    const ptr = wasm.torque_alloc(payload.length);
    new Uint8Array(wasm.memory.buffer).set(payload, ptr);
    const outPtr = wasm.torque_compile(ptr, payload.length);
    const outLen = wasm.torque_result_len();
    const output = decoder.decode(new Uint8Array(wasm.memory.buffer, outPtr, outLen).slice());
    wasm.torque_free(ptr, payload.length);
    const parsed = JSON.parse(output) as { files: CompilerFile[] };
    return parsed.files;
  } catch (error) {
    wasmExports = undefined;
    throw error;
  }
}

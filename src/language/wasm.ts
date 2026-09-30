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

import { readFileSync } from "node:fs";
import * as path from "node:path";
import { compileJson, initSync } from "torque-compiler";
import torqueCompilerWasm from "torque-compiler/torque_compiler_bg.wasm";

export type CompilerDiagnostic = {
  message: string;
  start: number;
  end: number;
  severity: "error" | "warning";
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
  builtinTypes?: string[];
};

initSync({ module: readFileSync(path.resolve(__dirname, torqueCompilerWasm)) });

let lastParseCountValue = 0;
let lastInputFileCountValue = 0;

export function lastCompileParseCount(): number {
  return lastParseCountValue;
}

export function lastCompileInputFileCount(): number {
  return lastInputFileCountValue;
}

export function compileSources(
  files: Array<{ uri: string; text: string }>,
  checkUris?: readonly string[],
  incremental = false,
): CompilerFile[] {
  lastInputFileCountValue = files.length;
  const parsed = JSON.parse(
    compileJson(
      JSON.stringify({
        files,
        ...(checkUris === undefined ? {} : { checkUris }),
        ...(incremental ? { incremental: true } : {}),
      }),
    ),
  ) as { files: CompilerFile[]; parseCount?: number };
  lastParseCountValue = parsed.parseCount ?? parsed.files.length;
  return parsed.files;
}

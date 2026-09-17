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

import { existsSync, readdirSync, statSync } from "node:fs";
import * as path from "node:path";

const SKIP_DIRS = new Set([".git", "node_modules", "out", "build", "dist"]);

export function findTorqueRoot(startPath: string): string | undefined {
  let current = startPath;
  try {
    if (statSync(current).isFile()) {
      current = path.dirname(current);
    }
  } catch {
    current = path.dirname(current);
  }
  let fallback: string | undefined;
  while (true) {
    const builtins = path.join(current, "src", "builtins");
    const objects = path.join(current, "src", "objects");
    if (existsSync(builtins) && existsSync(objects)) {
      return current;
    }
    if (existsSync(path.join(current, "src")) && existsSync(path.join(current, "BUILD.gn"))) {
      fallback = current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      break;
    }
    current = parent;
  }
  return fallback;
}

export function listTorqueFiles(root: string): string[] {
  const files: string[] = [];
  const roots = [
    path.join(root, "src"),
    path.join(root, "test", "torque"),
    path.join(root, "third_party", "v8"),
  ];
  for (const dir of roots) {
    if (existsSync(dir)) {
      walkTorqueFiles(dir, files);
    }
  }
  files.sort();
  return files;
}

function walkTorqueFiles(dir: string, out: string[]): void {
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    if (SKIP_DIRS.has(name)) {
      continue;
    }
    const full = path.join(dir, name);
    let stat;
    try {
      stat = statSync(full);
    } catch {
      continue;
    }
    if (stat.isDirectory()) {
      walkTorqueFiles(full, out);
      continue;
    }
    if (name.endsWith(".tq")) {
      out.push(full);
    }
  }
}

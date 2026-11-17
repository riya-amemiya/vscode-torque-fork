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

import { existsSync, statSync } from "node:fs";
import { readdir, stat } from "node:fs/promises";
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

export async function listTorqueFiles(root: string): Promise<string[]> {
  const roots = [
    path.join(root, "src"),
    path.join(root, "test", "torque"),
    path.join(root, "third_party", "v8"),
  ];
  const found = await Promise.all(roots.map((dir) => walkTorqueFiles(dir)));
  return found.flat().toSorted();
}

async function walkTorqueFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir).catch((): string[] => []);
  const found = await Promise.all(
    entries
      .filter((name) => !SKIP_DIRS.has(name))
      .map(async (name): Promise<string[]> => {
        const full = path.join(dir, name);
        const info = await stat(full).catch(() => undefined);
        if (info === undefined) {
          return [];
        }
        if (info.isDirectory()) {
          return walkTorqueFiles(full);
        }
        return name.endsWith(".tq") ? [full] : [];
      }),
  );
  return found.flat();
}

import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";
import { findTorqueRoot, listTorqueFiles } from "./workspace-files";

describe("findTorqueRoot", () => {
  test("walks up from src/builtins to the V8-style root", () => {
    const root = mkdtempSync(join(tmpdir(), "torque-root-"));
    mkdirSync(join(root, "src", "builtins"), { recursive: true });
    mkdirSync(join(root, "src", "objects"), { recursive: true });
    const builtin = join(root, "src", "builtins", "array-flat.tq");
    writeFileSync(builtin, "macro Flatten(): void {}\n");
    writeFileSync(join(root, "src", "objects", "js-array.tq"), "extern class JSArray {}\n");
    expect(findTorqueRoot(builtin)).toBe(root);
    expect(listTorqueFiles(root)).toEqual([
      join(root, "src", "builtins", "array-flat.tq"),
      join(root, "src", "objects", "js-array.tq"),
    ]);
  });
});

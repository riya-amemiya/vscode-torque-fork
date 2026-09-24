import { copyFileSync, mkdirSync } from "node:fs";
import * as path from "node:path";

const formatterWasm = path.join(
  process.cwd(),
  "node_modules",
  "@wasm-fmt",
  "clang-format",
  "clang-format.wasm",
);

const source = path.join(
  process.cwd(),
  "target",
  "wasm32-unknown-unknown",
  "release",
  "torque_compiler.wasm",
);
const destDir = path.join(process.cwd(), "dist");
mkdirSync(destDir, { recursive: true });
copyFileSync(source, path.join(destDir, "torque_compiler.wasm"));
copyFileSync(formatterWasm, path.join(destDir, "clang-format.wasm"));

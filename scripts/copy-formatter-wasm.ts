import { copyFileSync, mkdirSync } from "node:fs";
import * as path from "node:path";

const source = path.join(
  process.cwd(),
  "node_modules",
  "@wasm-fmt",
  "clang-format",
  "clang-format.wasm",
);
const destDir = path.join(process.cwd(), "dist");
mkdirSync(destDir, { recursive: true });
copyFileSync(source, path.join(destDir, "clang-format.wasm"));

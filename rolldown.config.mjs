import { defineConfig } from "rolldown";

export default defineConfig({
  input: "src/extension.ts",
  platform: "node",
  external: ["vscode"],
  output: {
    file: "dist/extension.js",
    format: "cjs",
    sourcemap: true,
    exports: "named",
  },
});

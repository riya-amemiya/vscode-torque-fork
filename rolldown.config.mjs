import { defineConfig } from "rolldown";

const minify = process.argv.includes("--minify");

export default defineConfig({
  input: "src/extension.ts",
  platform: "node",
  external: ["vscode"],
  output: {
    file: "dist/extension.js",
    format: "cjs",
    sourcemap: !minify,
    exports: "named",
    minify,
    comments: !minify,
  },
});

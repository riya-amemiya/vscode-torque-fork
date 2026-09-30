// Copyright 2014 the V8 project authors. All rights reserved.
// Copyright 2026 Riya Amemiya.
//
// The preprocessing in this file is a TypeScript port of V8's
// tools/torque/format-torque.py, which is licensed under the BSD-style
// license in the V8 repository. Modifications are licensed under the
// Apache License, Version 2.0.

import { readFileSync } from "node:fs";
import * as path from "node:path";
import clangFormatWasm from "@wasm-fmt/clang-format/wasm";
import { format as formatWithClang, initSync } from "@wasm-fmt/clang-format/web";

initSync(readFileSync(path.resolve(__dirname, clangFormatWasm)));

const PERCENT = "α";
const DEREF = "☆";
const ADDRESS = "⌂";

const CLANG_STYLE = JSON.stringify({
  BasedOnStyle: "Google",
  NamespaceIndentation: "None",
});

function dashes(count: number): string {
  return "-".repeat(Math.max(0, count));
}

function functionReplacement(keyword: string): string {
  const torqueDef = keyword.replaceAll(/\s+/g, " ");
  const functionLen = "function".length;
  const functionAndCommentLen = "function /**/".length;
  if (torqueDef.length < functionLen) {
    return `// !!torquefunc ${torqueDef}\nfunction `;
  }
  const pad = dashes(torqueDef.length - functionAndCommentLen);
  return `// !!torquefunc ${torqueDef}\nfunction /*${pad}*/ `;
}

function classReplacement(keyword: string): string {
  const torqueDef = keyword.replaceAll(/\s+/g, " ");
  const classLen = "class".length;
  const classAndCommentLen = "class /**/".length;
  if (torqueDef.length < classLen) {
    return `// !!torqueclass ${torqueDef}\nclass `;
  }
  const pad = dashes(torqueDef.length - classAndCommentLen);
  return `// !!torqueclass ${torqueDef}\nclass /*${pad}*/ `;
}

export function preprocessTorque(input: string): string {
  let text = input;
  text = text.replaceAll(/%([A-Za-z])/g, `${PERCENT}$1`);
  text = text.replaceAll(/([^/])\*([a-zA-Z(])/g, `$1${DEREF}$2`);
  text = text.replaceAll(/&([a-zA-Z(])/g, `${ADDRESS}$1`);
  text = text.replaceAll(/(if\s+)constexpr(\s*\()/g, "$1/*COxp*/$2");
  text = text.replaceAll(/\btypeswitch\s*(\([^{]*\))\s{/g, " if /*tPsW*/ $1 {");
  text = text.replaceAll(/\bcase\s*(\([^{]*\))\s*:\s*deferred\s*{/g, " if /*cAsEdEfF*/ $1 {");
  text = text.replaceAll(/\bcase\s*(\([^{]*\))\s*:\s*{/g, " if /*cA*/ $1 {");
  text = text.replaceAll(/\bgenerates\s+'([^']+)'\s*/g, "_GeNeRaT_/*$1@*/");
  text = text.replaceAll(/\bconstexpr\s+'([^']+)'\s*/g, "_CoNsExP_/*$1@*/");
  text = text.replaceAll(
    /^[ \t]*((?:extern\s+)?(?:transitioning\s+)?(?:javascript\s+)?(?:operator\s*'[^']+'\s*)?(?:macro|builtin|runtime))\s+/gm,
    (_match, keyword: string) => functionReplacement(keyword),
  );
  text = text.replaceAll(
    /^[ \t]*((?:extern\s+)?(?:bitfield\s+)?(?:struct|class|shape))\s+/gm,
    (_match, keyword: string) => classReplacement(keyword),
  );
  text = text.replaceAll(/\notherwise/g, "\n otherwise");
  text = text.replaceAll(/(\n\s*\S[^\n]*\s)otherwise/g, "$1_OtheSaLi");
  text = text.replaceAll(/@if(not)?\(/g, "if /*!if$1*/ (");
  text = text.replaceAll(/(js-)?implicit\s+([^)]*?)\s*\)\(\s*\)/gs, "/*$1ImPl*/$2Ǝ)");
  text = text.replaceAll(/(js-)?implicit\s+([^)]*?)\s*\)\(\s*/gs, "/*$1ImPl*/$2Ǝ,");
  text = text.replaceAll(/^(\s*)@([a-zA-Z]+)(\([^)]*\))?\n/gm, "$1//@$2$3\n");
  text = text.replaceAll(/^(\s*)@export\b/gm, "$1//@eXpOrT");
  text = text.replaceAll(/^#include/gm, "// InClUdE");
  return text;
}

export function postprocessTorque(output: string): string {
  let text = output;
  text = text.replaceAll(/^(\s*)\/\/@eXpOrT\b/gm, "$1@export");
  text = text.replaceAll(/^(\s*)\/\/@([a-zA-Z]+)(\([^)]*\))?\n/gm, "$1@$2$3\n");
  text = text.replaceAll(/\/\*COxp\*\//g, "constexpr");
  text = text.replaceAll(/(\S+)\s*: type([,>])/g, "$1: type$2");
  text = text.replaceAll(/(\n\s*)labels( [A-Z])/g, "$1 labels$2");
  text = text.replaceAll(/\bif\s*\/\*tPsW\*\//g, "typeswitch");
  text = text.replaceAll(/\bif\s*\/\*cA\*\/\s*(\([^{]*\))\s*{/g, "case $1: {");
  text = text.replaceAll(/\bif\s*\/\*cAsEdEfF\*\/\s*(\([^{]*\))\s*{/g, "case $1: deferred {");
  text = text.replaceAll(/\n_GeNeRaT_\s*\/\*([^@]+)@\*\//g, "\n generates '$1'");
  text = text.replaceAll(/_GeNeRaT_\s*\/\*([^@]+)@\*\//g, "generates '$1'");
  text = text.replaceAll(/\n_CoNsExP_\s*\/\*([^@]+)@\*\//g, "\n constexpr '$1'");
  text = text.replaceAll(/_CoNsExP_\s*\/\*([^@]+)@\*\//g, "constexpr '$1'");
  text = text.replaceAll(/\/\/ !!torqueclass (.*)\n\s*class(?:\s*\/\*-*\*\/)?/g, "$1");
  text = text.replaceAll(/\/\/ !!torquefunc (.*)\n\s*function(?:\s*\/\*-*\*\/)?/g, "$1");
  text = text.replaceAll(/\n(\s+)otherwise/g, "\n$1 otherwise");
  text = text.replaceAll(/\n(\s+)_OtheSaLi/g, "\n$1otherwise");
  text = text.replaceAll(/_OtheSaLi/g, "otherwise");
  text = text.replaceAll(/if\s*\/\*!if(not)?\*\/\s*\(/g, "@if$1(");
  text = text.replaceAll(/\/\*\s*(js-)?ImPl\s*\*\/\s*([^Ǝ]*?)Ǝ\s*\)/gs, "$1implicit $2)()");
  text = text.replaceAll(/\/\*\s*(js-)?ImPl\s*\*\/\s*([^Ǝ]*?)Ǝ\s*, */gs, "$1implicit $2)(");
  text = text.replaceAll(/}\n *label /g, "} label ");
  text = text.replaceAll(PERCENT, "%");
  text = text.replaceAll(DEREF, "*");
  text = text.replaceAll(ADDRESS, "&");
  text = text.replaceAll(/^\/\/ InClUdE/gm, "#include");
  return text;
}

export function formatTorque(source: string): string {
  const formatted = formatWithClang(preprocessTorque(source), "main.ts", CLANG_STYLE);
  return postprocessTorque(formatted);
}

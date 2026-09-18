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

import { resolveDefinition, type DocumentAnalysis, type TorqueSymbolKind } from "./analyze";
import { TORQUE_BUILTINS, TORQUE_COMMON_TYPES } from "./keywords";
import type { Token } from "./lexer";
import { rangeFromOffsets } from "./positions";

export const SEMANTIC_TOKEN_TYPES = [
  "namespace",
  "class",
  "enum",
  "struct",
  "type",
  "variable",
  "property",
  "function",
  "macro",
  "label",
] as const;

export const SEMANTIC_TOKEN_MODIFIERS = ["declaration", "readonly", "defaultLibrary"] as const;

export type SemanticTokenType = (typeof SEMANTIC_TOKEN_TYPES)[number];
export type SemanticTokenModifier = (typeof SEMANTIC_TOKEN_MODIFIERS)[number];

export type SemanticSpan = {
  line: number;
  character: number;
  length: number;
  type: SemanticTokenType;
  modifiers: SemanticTokenModifier[];
};

const VALUE_NAMES: ReadonlySet<string> = new Set([
  "True",
  "False",
  "Null",
  "Undefined",
  "Hole",
  "true",
  "false",
]);

const BUILTIN_TYPES: ReadonlySet<string> = new Set(
  [...TORQUE_COMMON_TYPES, "Tagged", "Arguments", "MaybeObject"].filter(
    (name) => !VALUE_NAMES.has(name),
  ),
);

const BUILTIN_FUNCTIONS: ReadonlySet<string> = new Set(TORQUE_BUILTINS.map((item) => item.name));

function typeFor(kind: TorqueSymbolKind): SemanticTokenType {
  switch (kind) {
    case "namespace":
      return "namespace";
    case "class":
    case "shape":
      return "class";
    case "enum":
      return "enum";
    case "struct":
      return "struct";
    case "type":
      return "type";
    case "field":
      return "property";
    case "macro":
      return "macro";
    case "builtin":
    case "runtime":
    case "intrinsic":
      return "function";
    case "const":
    case "let":
      return "variable";
    case "label":
      return "label";
    default: {
      const exhaustive: never = kind;
      return exhaustive;
    }
  }
}

function modifiersFor(
  kind: TorqueSymbolKind,
  declaration: boolean,
  library: boolean,
): SemanticTokenModifier[] {
  const modifiers: SemanticTokenModifier[] = [];
  if (declaration) {
    modifiers.push("declaration");
  }
  if (kind === "const") {
    modifiers.push("readonly");
  }
  if (library) {
    modifiers.push("defaultLibrary");
  }
  return modifiers;
}

function classificationFor(
  analysis: DocumentAnalysis,
  token: Token,
  workspace: readonly DocumentAnalysis[],
): { type: SemanticTokenType; modifiers: SemanticTokenModifier[] } | undefined {
  if (
    token.kind === "comment" ||
    token.kind === "string" ||
    token.kind === "error" ||
    token.kind === "punct" ||
    token.kind === "include" ||
    token.kind === "annotation" ||
    token.kind === "number"
  ) {
    return undefined;
  }
  if (token.kind === "keyword") {
    if (token.text === "void" || token.text === "never") {
      return { type: "type", modifiers: ["defaultLibrary"] };
    }
    return undefined;
  }
  const declared = analysis.symbols.find(
    (symbol) => symbol.start === token.start && symbol.end === token.end,
  );
  if (declared !== undefined) {
    return {
      type: typeFor(declared.kind),
      modifiers: modifiersFor(declared.kind, true, false),
    };
  }
  const resolved = resolveDefinition(analysis, token.start, workspace)[0];
  if (resolved !== undefined) {
    return {
      type: typeFor(resolved.kind),
      modifiers: modifiersFor(resolved.kind, false, false),
    };
  }
  if (BUILTIN_TYPES.has(token.text)) {
    return { type: "type", modifiers: ["defaultLibrary"] };
  }
  if (BUILTIN_FUNCTIONS.has(token.text)) {
    return { type: "function", modifiers: ["defaultLibrary"] };
  }
  return undefined;
}

export function semanticTokensFor(
  analysis: DocumentAnalysis,
  workspace: readonly DocumentAnalysis[] = [],
): SemanticSpan[] {
  const spans: SemanticSpan[] = [];
  for (const token of analysis.tokens) {
    const classified = classificationFor(analysis, token, workspace);
    if (classified === undefined) {
      continue;
    }
    const range = rangeFromOffsets(analysis.lines, token.start, token.end);
    if (range.start.line !== range.end.line) {
      continue;
    }
    spans.push({
      line: range.start.line,
      character: range.start.character,
      length: token.end - token.start,
      type: classified.type,
      modifiers: classified.modifiers,
    });
  }
  return spans;
}

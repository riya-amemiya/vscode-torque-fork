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
import { TORQUE_BUILTINS } from "./keywords";
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

const BUILTIN_FUNCTIONS: ReadonlySet<string> = new Set(TORQUE_BUILTINS.map((item) => item.name));

const GENERIC_INNER = new Set([",", ":", "|", "&", "::", "."]);

const TYPE_INTRODUCERS = new Set([
  ":",
  "extends",
  "type",
  "class",
  "struct",
  "enum",
  "shape",
  "new",
]);

function isTypeExprToken(token: Token): boolean {
  if (token.kind === "identifier") {
    return true;
  }
  if (token.text === "|" || token.text === "&" || token.text === "::" || token.text === ".") {
    return true;
  }
  return (
    token.text === "constexpr" ||
    token.text === "weak" ||
    token.text === "void" ||
    token.text === "never"
  );
}

function lastNonTypeExpr(tokens: readonly Token[], index: number): number {
  if (index < 0) {
    return -1;
  }
  if (isTypeExprToken(tokens[index])) {
    return lastNonTypeExpr(tokens, index - 1);
  }
  return index;
}

function typeKeywordBefore(tokens: readonly Token[], index: number): boolean {
  if (index < 0) {
    return false;
  }
  const token = tokens[index];
  if (token.text === ";" || token.text === "{") {
    return false;
  }
  if (token.text === "type") {
    return true;
  }
  if (
    token.kind === "identifier" ||
    token.text === "<" ||
    token.text === ">" ||
    token.text === "," ||
    token.text === ":" ||
    token.text === "extends" ||
    token.text === "constexpr"
  ) {
    return typeKeywordBefore(tokens, index - 1);
  }
  return false;
}

function isGenericInnerToken(token: Token): boolean {
  if (token.kind === "identifier" || token.kind === "keyword") {
    return true;
  }
  return GENERIC_INNER.has(token.text);
}

function findOpenAngle(
  tokens: readonly Token[],
  index: number,
  extraCloses: number,
): number | undefined {
  if (index < 0) {
    return undefined;
  }
  const token = tokens[index];
  if (token.text === ";" || token.text === "{" || token.text === "}") {
    return undefined;
  }
  if (token.text === "<") {
    if (extraCloses === 0) {
      return index;
    }
    return findOpenAngle(tokens, index - 1, extraCloses - 1);
  }
  if (token.text === ">") {
    return findOpenAngle(tokens, index - 1, extraCloses + 1);
  }
  return findOpenAngle(tokens, index - 1, extraCloses);
}

function scanGeneric(tokens: readonly Token[], index: number, extraOpens: number): boolean {
  if (index >= tokens.length) {
    return false;
  }
  const token = tokens[index];
  if (token.text === ";" || token.text === "{") {
    return false;
  }
  if (token.text === "<") {
    return scanGeneric(tokens, index + 1, extraOpens + 1);
  }
  if (token.text === ">") {
    if (extraOpens === 0) {
      return true;
    }
    return scanGeneric(tokens, index + 1, extraOpens - 1);
  }
  if (!isGenericInnerToken(token)) {
    return false;
  }
  return scanGeneric(tokens, index + 1, extraOpens);
}

function insideGenericArgs(tokens: readonly Token[], index: number): boolean {
  const open = findOpenAngle(tokens, index - 1, 0);
  if (open === undefined || open === 0) {
    return false;
  }
  const before = tokens[open - 1];
  if (before === undefined) {
    return false;
  }
  if (before.kind !== "identifier" && before.kind !== "keyword") {
    return false;
  }
  return scanGeneric(tokens, index, 0);
}

function isTypeKind(kind: TorqueSymbolKind): boolean {
  return (
    kind === "type" || kind === "class" || kind === "struct" || kind === "enum" || kind === "shape"
  );
}

function isKnownTypeName(
  analysis: DocumentAnalysis,
  workspace: readonly DocumentAnalysis[],
  name: string,
): boolean {
  if (analysis.builtinTypes.includes(name)) {
    return true;
  }
  return [analysis, ...workspace].some((document) =>
    document.symbols.some((symbol) => symbol.name === name && isTypeKind(symbol.kind)),
  );
}

function inTypePosition(tokens: readonly Token[], index: number): boolean {
  if (insideGenericArgs(tokens, index)) {
    return true;
  }
  const intro = lastNonTypeExpr(tokens, index - 1);
  if (intro < 0) {
    return false;
  }
  const token = tokens[intro];
  if (TYPE_INTRODUCERS.has(token.text)) {
    return true;
  }
  return token.text === "=" && typeKeywordBefore(tokens, intro - 1);
}

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
  const index = analysis.tokens.indexOf(token);
  if (
    index >= 0 &&
    token.kind === "identifier" &&
    !VALUE_NAMES.has(token.text) &&
    inTypePosition(analysis.tokens, index) &&
    isKnownTypeName(analysis, workspace, token.text)
  ) {
    const library = analysis.builtinTypes.includes(token.text);
    return { type: "type", modifiers: library ? ["defaultLibrary"] : [] };
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

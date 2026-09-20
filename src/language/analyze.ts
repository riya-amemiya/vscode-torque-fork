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

import { tokenize, type Token } from "./lexer";
import { createLineTable, rangeFromOffsets, type LineTable, type Range } from "./positions";
import { compileSources, type CompilerFile, type CompilerSymbol } from "./wasm";

export type TorqueSymbolKind =
  | "builtin"
  | "class"
  | "const"
  | "enum"
  | "field"
  | "intrinsic"
  | "label"
  | "let"
  | "macro"
  | "namespace"
  | "runtime"
  | "shape"
  | "struct"
  | "type";

const SYMBOL_KINDS: ReadonlySet<string> = new Set([
  "builtin",
  "class",
  "const",
  "enum",
  "field",
  "intrinsic",
  "label",
  "let",
  "macro",
  "namespace",
  "runtime",
  "shape",
  "struct",
  "type",
]);

export type TorqueSymbol = {
  name: string;
  kind: TorqueSymbolKind;
  start: number;
  end: number;
  containerName?: string;
  detail?: string;
};

export type TorqueDiagnostic = {
  message: string;
  start: number;
  end: number;
  severity: "error";
};

export type IncludeReference = {
  path: string;
  start: number;
  end: number;
};

export type DefinitionMapping = {
  fromStart: number;
  fromEnd: number;
  toUri: string;
  toStart: number;
  toEnd: number;
};

export type DocumentAnalysis = {
  uri: string;
  text: string;
  tokens: Token[];
  symbols: TorqueSymbol[];
  diagnostics: TorqueDiagnostic[];
  includes: IncludeReference[];
  definitions: DefinitionMapping[];
  builtinTypes: string[];
  lines: LineTable;
};

function asSymbolKind(kind: string): TorqueSymbolKind {
  if (SYMBOL_KINDS.has(kind)) {
    return kind as TorqueSymbolKind;
  }
  return "const";
}

function toSymbol(symbol: CompilerSymbol): TorqueSymbol {
  return {
    name: symbol.name,
    kind: asSymbolKind(symbol.kind),
    start: symbol.start,
    end: symbol.end,
    containerName: symbol.containerName,
    detail: symbol.detail,
  };
}

export function analysisFromCompiler(
  uri: string,
  text: string,
  file: CompilerFile,
): DocumentAnalysis {
  return {
    uri,
    text,
    tokens: tokenize(text),
    symbols: file.symbols.map(toSymbol),
    diagnostics: file.diagnostics.map((item) => ({
      message: item.message,
      start: item.start,
      end: item.end,
      severity: "error",
    })),
    includes: file.includes.map((item) => ({
      path: item.path,
      start: item.start,
      end: item.end,
    })),
    definitions: file.definitions.map((item) => ({
      fromStart: item.fromStart,
      fromEnd: item.fromEnd,
      toUri: item.toUri,
      toStart: item.toStart,
      toEnd: item.toEnd,
    })),
    builtinTypes: file.builtinTypes ?? [],
    lines: createLineTable(text),
  };
}

export function analyzeDocuments(
  files: Array<{ uri: string; text: string }>,
  checkUris?: readonly string[],
  incremental = false,
): Map<string, DocumentAnalysis> {
  const compiled = compileSources(files, checkUris, incremental);
  const analyses = new Map<string, DocumentAnalysis>();
  for (const file of compiled) {
    const source = files.find((item) => item.uri === file.uri)?.text ?? "";
    analyses.set(file.uri, analysisFromCompiler(file.uri, source, file));
  }
  return analyses;
}

export function analyzeDocument(text: string, uri = "memory://document.tq"): DocumentAnalysis {
  const files = analyzeDocuments([{ uri, text }]);
  return files.get(uri) ?? analysisFromCompiler(uri, text, emptyCompilerFile(uri));
}

const TYPE_SYMBOL_KINDS: ReadonlySet<TorqueSymbolKind> = new Set([
  "type",
  "class",
  "struct",
  "enum",
  "shape",
]);

function unresolvedName(message: string): string | undefined {
  const prefix = "Cannot resolve '";
  if (!message.startsWith(prefix) || !message.endsWith("'")) {
    return undefined;
  }
  return message.slice(prefix.length, message.length - 1);
}

function unresolvedTypeName(message: string): string | undefined {
  const prefix = "Cannot resolve type '";
  if (!message.startsWith(prefix) || !message.endsWith("'")) {
    return undefined;
  }
  return message.slice(prefix.length, message.length - 1);
}

function matchingCallableName(message: string): string | undefined {
  const prefix = "Cannot find matching callable '";
  if (!message.startsWith(prefix) || !message.endsWith("'")) {
    return undefined;
  }
  return message.slice(prefix.length, message.length - 1);
}

function isSharedConst(symbol: TorqueSymbol): boolean {
  if (symbol.containerName !== undefined) {
    return true;
  }
  return symbol.name.startsWith("k") && symbol.name.length > 1;
}

function qualifierAt(analysis: DocumentAnalysis, start: number): string | undefined {
  const tokens = analysis.tokens.filter((token) => token.kind !== "comment");
  const index = tokens.findIndex((token) => start >= token.start && start < token.end);
  if (index < 2) {
    return undefined;
  }
  const operator = tokens[index - 1];
  const namespace = tokens[index - 2];
  if (operator.text !== "::") {
    return undefined;
  }
  if (namespace.kind !== "identifier" && namespace.kind !== "keyword") {
    return undefined;
  }
  return namespace.text;
}

export function dropResolvedElsewhere(
  analysis: DocumentAnalysis,
  workspace: readonly DocumentAnalysis[],
): DocumentAnalysis {
  const knownQualified = new Set<string>();
  const knownTypes = new Set<string>();
  const knownValues = new Set<string>();
  const knownCallables = new Set<string>();
  for (const document of workspace) {
    for (const symbol of document.symbols) {
      if (symbol.containerName !== undefined) {
        knownQualified.add(`${symbol.containerName}::${symbol.name}`);
      }
      if (TYPE_SYMBOL_KINDS.has(symbol.kind)) {
        knownTypes.add(symbol.name);
      }
      if (
        symbol.kind === "macro" ||
        symbol.kind === "builtin" ||
        symbol.kind === "runtime" ||
        symbol.kind === "intrinsic"
      ) {
        knownCallables.add(symbol.name);
        knownValues.add(symbol.name);
      }
      if (symbol.kind === "namespace") {
        knownValues.add(symbol.name);
      }
      if (symbol.kind === "const" && isSharedConst(symbol)) {
        knownValues.add(symbol.name);
      }
    }
  }
  return {
    ...analysis,
    diagnostics: analysis.diagnostics.filter((item) => {
      const typeName = unresolvedTypeName(item.message);
      if (typeName !== undefined) {
        return !knownTypes.has(typeName);
      }
      const callable = matchingCallableName(item.message);
      if (callable !== undefined) {
        return !knownCallables.has(callable);
      }
      const name = unresolvedName(item.message);
      if (name === undefined) {
        return true;
      }
      const qualifier = qualifierAt(analysis, item.start);
      if (qualifier !== undefined) {
        return !knownQualified.has(`${qualifier}::${name}`);
      }
      return !knownValues.has(name);
    }),
  };
}

function emptyCompilerFile(uri: string): CompilerFile {
  return {
    uri,
    diagnostics: [],
    symbols: [],
    includes: [],
    definitions: [],
    builtinTypes: [],
  };
}

export function diagnosticRange(analysis: DocumentAnalysis, diagnostic: TorqueDiagnostic): Range {
  return rangeFromOffsets(analysis.lines, diagnostic.start, diagnostic.end);
}

export function symbolRange(analysis: DocumentAnalysis, symbol: TorqueSymbol): Range {
  return rangeFromOffsets(analysis.lines, symbol.start, symbol.end);
}

export function definitionRange(analysis: DocumentAnalysis, definition: DefinitionMapping): Range {
  return rangeFromOffsets(analysis.lines, definition.toStart, definition.toEnd);
}

function isIdentLike(token: Token): boolean {
  return token.kind === "identifier" || token.kind === "keyword" || token.kind === "string";
}

export function identifierAt(analysis: DocumentAnalysis, offset: number): Token | undefined {
  const contained = analysis.tokens.find((token) => offset >= token.start && offset < token.end);
  if (contained !== undefined && isIdentLike(contained)) {
    return contained;
  }
  let atEnd: Token | undefined;
  for (const token of analysis.tokens) {
    if (token.end === offset && isIdentLike(token)) {
      atEnd = token;
    }
  }
  return atEnd;
}

function definitionAt(analysis: DocumentAnalysis, offset: number): DefinitionMapping[] {
  return analysis.definitions.filter((item) => offset >= item.fromStart && offset <= item.fromEnd);
}

function symbolAt(
  analysis: DocumentAnalysis,
  start: number,
  end: number,
): TorqueSymbol | undefined {
  return analysis.symbols.find((symbol) => symbol.start === start && symbol.end === end);
}

function symbolNameMatches(symbol: TorqueSymbol, name: string): boolean {
  return symbol.name === name || symbol.name.endsWith(`::${name}`);
}

function resolveDefinitionByName(
  analysis: DocumentAnalysis,
  offset: number,
  workspace: readonly DocumentAnalysis[],
): TorqueSymbol[] {
  const token = identifierAt(analysis, offset);
  if (token === undefined || token.kind === "string") {
    return [];
  }
  const name = token.text;
  const documents = [analysis, ...workspace.filter((item) => item !== analysis)];
  const matches: TorqueSymbol[] = [];
  for (const document of documents) {
    for (const symbol of document.symbols) {
      if (symbolNameMatches(symbol, name)) {
        matches.push(symbol);
      }
    }
  }
  const covering = matches.filter(
    (symbol) => analysis.symbols.includes(symbol) && offset >= symbol.start && offset <= symbol.end,
  );
  if (covering.length > 0) {
    return [covering[covering.length - 1]];
  }
  const local = matches.filter(
    (symbol) => analysis.symbols.includes(symbol) && symbol.end <= token.start,
  );
  if (local.length > 0) {
    return [local[local.length - 1]];
  }
  const sameFile = matches.filter((symbol) => analysis.symbols.includes(symbol));
  if (sameFile.length > 0) {
    return [sameFile[0]];
  }
  return matches.slice(0, 8);
}

export function resolveDefinition(
  analysis: DocumentAnalysis,
  offset: number,
  workspace: readonly DocumentAnalysis[],
): TorqueSymbol[] {
  const hits = definitionAt(analysis, offset);
  const resolved: TorqueSymbol[] = [];
  for (const hit of hits) {
    const target =
      hit.toUri === analysis.uri
        ? analysis
        : (workspace.find((item) => item.uri === hit.toUri) ?? analysis);
    const symbol = symbolAt(target, hit.toStart, hit.toEnd);
    if (symbol !== undefined) {
      resolved.push(symbol);
      continue;
    }
    const named = target.symbols.find(
      (candidate) =>
        candidate.start === hit.toStart ||
        (candidate.start <= hit.toStart && candidate.end >= hit.toEnd),
    );
    if (named !== undefined) {
      resolved.push(named);
    }
  }
  if (resolved.length > 0) {
    return resolved;
  }
  return resolveDefinitionByName(analysis, offset, workspace);
}

export function includeAt(
  analysis: DocumentAnalysis,
  offset: number,
): IncludeReference | undefined {
  return analysis.includes.find((item) => offset >= item.start && offset <= item.end);
}

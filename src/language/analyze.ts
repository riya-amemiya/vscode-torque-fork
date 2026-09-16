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

export type TorqueSymbolKind =
  | "builtin"
  | "class"
  | "const"
  | "enum"
  | "field"
  | "intrinsic"
  | "macro"
  | "namespace"
  | "runtime"
  | "shape"
  | "struct"
  | "type";

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

export type DocumentAnalysis = {
  text: string;
  tokens: Token[];
  symbols: TorqueSymbol[];
  diagnostics: TorqueDiagnostic[];
  includes: IncludeReference[];
  lines: LineTable;
};

const OPEN_TO_CLOSE: Record<string, string> = {
  "(": ")",
  "[": "]",
  "{": "}",
};

const CALLABLE_KINDS: Record<string, TorqueSymbolKind> = {
  builtin: "builtin",
  intrinsic: "intrinsic",
  macro: "macro",
  runtime: "runtime",
};

const TYPE_KINDS: Record<string, TorqueSymbolKind> = {
  class: "class",
  enum: "enum",
  shape: "shape",
  struct: "struct",
  type: "type",
};

function isTrivia(token: Token): boolean {
  return token.kind === "comment";
}

class TokenCursor {
  readonly tokens: Token[];
  index = 0;

  constructor(tokens: Token[]) {
    this.tokens = tokens;
  }

  peek(): Token | undefined {
    while (this.index < this.tokens.length && isTrivia(this.tokens[this.index])) {
      this.index += 1;
    }
    return this.tokens[this.index];
  }

  take(): Token | undefined {
    const token = this.peek();
    if (token !== undefined) {
      this.index += 1;
    }
    return token;
  }

  at(text: string): boolean {
    return this.peek()?.text === text;
  }

  eat(text: string): boolean {
    if (this.at(text)) {
      this.take();
      return true;
    }
    return false;
  }

  nextNonTrivia(): Token | undefined {
    this.peek();
    let index = this.index + 1;
    while (index < this.tokens.length && isTrivia(this.tokens[index])) {
      index += 1;
    }
    return this.tokens[index];
  }
}

function skipBalanced(cursor: TokenCursor, open: string, close: string): void {
  if (!cursor.eat(open)) {
    return;
  }
  let depth = 1;
  while (cursor.peek() !== undefined && depth > 0) {
    const token = cursor.take();
    if (token === undefined) {
      return;
    }
    if (token.text === open) {
      depth += 1;
    } else if (token.text === close) {
      depth -= 1;
    }
  }
}

function readQualifiedName(
  cursor: TokenCursor,
): { name: string; start: number; end: number } | undefined {
  const first = cursor.peek();
  if (first === undefined || (first.kind !== "identifier" && first.kind !== "keyword")) {
    return undefined;
  }
  cursor.take();
  let name = first.text;
  let end = first.end;
  while (cursor.eat("::")) {
    const next = cursor.peek();
    if (next === undefined || (next.kind !== "identifier" && next.kind !== "keyword")) {
      break;
    }
    cursor.take();
    name += `::${next.text}`;
    end = next.end;
  }
  return { name, start: first.start, end };
}

function callableName(
  cursor: TokenCursor,
): { name: string; start: number; end: number } | undefined {
  if (cursor.eat("operator")) {
    const next = cursor.take();
    if (next === undefined) {
      return undefined;
    }
    return { name: `operator${next.text}`, start: next.start, end: next.end };
  }
  return readQualifiedName(cursor);
}

function collectDelimiters(tokens: Token[]): TorqueDiagnostic[] {
  const diagnostics: TorqueDiagnostic[] = [];
  const stack: Token[] = [];
  for (const token of tokens) {
    if (token.kind !== "punct") {
      continue;
    }
    const closer = OPEN_TO_CLOSE[token.text];
    if (closer !== undefined) {
      stack.push(token);
      continue;
    }
    if (token.text === ")" || token.text === "]" || token.text === "}") {
      const open = stack.at(-1);
      if (open === undefined) {
        diagnostics.push({
          message: `Unmatched '${token.text}'`,
          start: token.start,
          end: token.end,
          severity: "error",
        });
        continue;
      }
      if (OPEN_TO_CLOSE[open.text] !== token.text) {
        diagnostics.push({
          message: `Expected '${OPEN_TO_CLOSE[open.text]}' but found '${token.text}'`,
          start: token.start,
          end: token.end,
          severity: "error",
        });
      }
      stack.pop();
    }
  }
  for (const open of stack) {
    diagnostics.push({
      message: `Unclosed '${open.text}'`,
      start: open.start,
      end: open.end,
      severity: "error",
    });
  }
  return diagnostics;
}

function parseBlock(
  cursor: TokenCursor,
  symbols: TorqueSymbol[],
  diagnostics: TorqueDiagnostic[],
  includes: IncludeReference[],
  containerName: string | undefined,
  mode: "top" | "type" | "code",
): void {
  while (cursor.peek() !== undefined && !cursor.at("}")) {
    const token = cursor.peek();
    if (token === undefined) {
      return;
    }

    if (token.kind === "error") {
      diagnostics.push({
        message: token.message ?? "Invalid syntax",
        start: token.start,
        end: token.end,
        severity: "error",
      });
      cursor.take();
      continue;
    }

    if (token.kind === "include") {
      cursor.take();
      const pathToken = cursor.peek();
      if (pathToken?.kind === "string") {
        cursor.take();
        includes.push({
          path: pathToken.text.slice(1, -1),
          start: pathToken.start + 1,
          end: pathToken.end - 1,
        });
      } else {
        diagnostics.push({
          message: "Expected string path after #include",
          start: token.start,
          end: token.end,
          severity: "error",
        });
      }
      continue;
    }

    if (token.kind === "annotation") {
      cursor.take();
      if (cursor.at("(")) {
        skipBalanced(cursor, "(", ")");
      }
      continue;
    }

    if (
      token.kind === "keyword" &&
      (token.text === "extern" ||
        token.text === "transient" ||
        token.text === "transitioning" ||
        token.text === "javascript" ||
        token.text === "constexpr" ||
        token.text === "weak")
    ) {
      cursor.take();
      continue;
    }

    if (token.kind === "keyword" && token.text === "namespace") {
      cursor.take();
      const name = readQualifiedName(cursor);
      if (name === undefined) {
        diagnostics.push({
          message: "Expected namespace name",
          start: token.start,
          end: token.end,
          severity: "error",
        });
        continue;
      }
      symbols.push({
        name: name.name,
        kind: "namespace",
        start: name.start,
        end: name.end,
        containerName,
      });
      if (cursor.eat("{")) {
        parseBlock(cursor, symbols, diagnostics, includes, name.name, "top");
        if (!cursor.eat("}")) {
          diagnostics.push({
            message: "Expected '}' to close namespace",
            start: name.start,
            end: name.end,
            severity: "error",
          });
        }
      }
      continue;
    }

    if (token.kind === "keyword" && token.text === "bitfield") {
      cursor.take();
      if (!cursor.eat("struct")) {
        diagnostics.push({
          message: "Expected 'struct' after 'bitfield'",
          start: token.start,
          end: token.end,
          severity: "error",
        });
        continue;
      }
      const name = readQualifiedName(cursor);
      if (name === undefined) {
        diagnostics.push({
          message: "Expected bitfield struct name",
          start: token.start,
          end: token.end,
          severity: "error",
        });
        continue;
      }
      symbols.push({
        name: name.name,
        kind: "struct",
        start: name.start,
        end: name.end,
        containerName,
        detail: "bitfield struct",
      });
      while (cursor.peek() !== undefined && !cursor.at("{") && !cursor.at(";")) {
        cursor.take();
      }
      if (cursor.eat("{")) {
        parseBlock(cursor, symbols, diagnostics, includes, name.name, "type");
        cursor.eat("}");
      } else {
        cursor.eat(";");
      }
      continue;
    }

    if (token.kind === "keyword" && token.text in TYPE_KINDS) {
      const kind = TYPE_KINDS[token.text];
      cursor.take();
      const name = readQualifiedName(cursor);
      if (name === undefined) {
        diagnostics.push({
          message: `Expected ${token.text} name`,
          start: token.start,
          end: token.end,
          severity: "error",
        });
        continue;
      }
      symbols.push({
        name: name.name,
        kind,
        start: name.start,
        end: name.end,
        containerName,
      });
      while (cursor.peek() !== undefined && !cursor.at("{") && !cursor.at(";")) {
        cursor.take();
      }
      if (cursor.eat("{")) {
        parseBlock(cursor, symbols, diagnostics, includes, name.name, "type");
        cursor.eat("}");
      } else {
        cursor.eat(";");
      }
      continue;
    }

    if (token.kind === "keyword" && token.text in CALLABLE_KINDS) {
      if (cursor.nextNonTrivia()?.text === "::") {
        cursor.take();
        continue;
      }
      const kind = CALLABLE_KINDS[token.text];
      cursor.take();
      const name = callableName(cursor);
      if (name === undefined) {
        diagnostics.push({
          message: `Expected ${token.text} name`,
          start: token.start,
          end: token.end,
          severity: "error",
        });
        continue;
      }
      const detailStart = name.end;
      while (cursor.peek() !== undefined && !cursor.at("{") && !cursor.at(";")) {
        cursor.take();
      }
      const beforeBody = cursor.peek();
      symbols.push({
        name: name.name,
        kind,
        start: name.start,
        end: name.end,
        containerName,
        detail:
          beforeBody === undefined
            ? undefined
            : cursor.tokens
                .filter((item) => item.start >= detailStart && item.end <= beforeBody.start)
                .map((item) => item.text)
                .join(" ")
                .trim() || undefined,
      });
      if (cursor.eat("{")) {
        parseBlock(cursor, symbols, diagnostics, includes, name.name, "code");
        cursor.eat("}");
      } else {
        cursor.eat(";");
      }
      continue;
    }

    if (token.kind === "keyword" && (token.text === "let" || token.text === "const")) {
      const kind = "const";
      cursor.take();
      const name = readQualifiedName(cursor);
      if (name === undefined) {
        diagnostics.push({
          message: `Expected identifier after '${token.text}'`,
          start: token.start,
          end: token.end,
          severity: "error",
        });
        continue;
      }
      symbols.push({
        name: name.name,
        kind,
        start: name.start,
        end: name.end,
        containerName,
      });
      while (cursor.peek() !== undefined && !cursor.at(";") && !cursor.at("{") && !cursor.at("}")) {
        cursor.take();
      }
      cursor.eat(";");
      continue;
    }

    if (token.kind === "identifier" && mode === "type" && containerName !== undefined) {
      const name = token;
      const saved = cursor.index;
      cursor.take();
      if (cursor.at(":")) {
        cursor.take();
        symbols.push({
          name: name.text,
          kind: "field",
          start: name.start,
          end: name.end,
          containerName,
        });
        while (
          cursor.peek() !== undefined &&
          !cursor.at(";") &&
          !cursor.at("{") &&
          !cursor.at("}")
        ) {
          cursor.take();
        }
        cursor.eat(";");
        continue;
      }
      cursor.index = saved;
    }

    if (cursor.eat("{")) {
      parseBlock(cursor, symbols, diagnostics, includes, containerName, mode);
      cursor.eat("}");
      continue;
    }

    cursor.take();
  }
}

export function analyzeDocument(text: string): DocumentAnalysis {
  const tokens = tokenize(text);
  const symbols: TorqueSymbol[] = [];
  const includes: IncludeReference[] = [];
  const diagnostics: TorqueDiagnostic[] = [
    ...tokens
      .filter((token) => token.kind === "error")
      .map((token) => ({
        message: token.message ?? "Invalid syntax",
        start: token.start,
        end: token.end,
        severity: "error" as const,
      })),
    ...collectDelimiters(tokens),
  ];
  parseBlock(new TokenCursor(tokens), symbols, diagnostics, includes, undefined, "top");
  const unique = new Map<string, TorqueDiagnostic>();
  for (const diagnostic of diagnostics) {
    unique.set(`${diagnostic.start}:${diagnostic.end}:${diagnostic.message}`, diagnostic);
  }
  const merged = [...unique.values()];
  merged.sort((left, right) => left.start - right.start);
  return {
    text,
    tokens,
    symbols,
    diagnostics: merged,
    includes,
    lines: createLineTable(text),
  };
}

export function diagnosticRange(analysis: DocumentAnalysis, diagnostic: TorqueDiagnostic): Range {
  return rangeFromOffsets(analysis.lines, diagnostic.start, diagnostic.end);
}

export function symbolRange(analysis: DocumentAnalysis, symbol: TorqueSymbol): Range {
  return rangeFromOffsets(analysis.lines, symbol.start, symbol.end);
}

export function identifierAt(analysis: DocumentAnalysis, offset: number): Token | undefined {
  const inclusive = analysis.tokens.find((token) => offset >= token.start && offset <= token.end);
  if (
    inclusive !== undefined &&
    (inclusive.kind === "identifier" || inclusive.kind === "keyword" || inclusive.kind === "string")
  ) {
    return inclusive;
  }
  let nearest: Token | undefined;
  for (const token of analysis.tokens) {
    if (token.end < offset) {
      nearest = token;
    } else if (token.start > offset) {
      break;
    }
  }
  if (
    nearest !== undefined &&
    (nearest.kind === "identifier" || nearest.kind === "keyword") &&
    offset - nearest.end <= 0
  ) {
    return nearest;
  }
  return undefined;
}

export function resolveDefinition(
  analysis: DocumentAnalysis,
  offset: number,
  workspace: readonly DocumentAnalysis[],
): TorqueSymbol[] {
  const token = identifierAt(analysis, offset);
  if (token === undefined || token.kind === "string") {
    return [];
  }
  const name = token.text;
  const matches: TorqueSymbol[] = [];
  for (const document of [analysis, ...workspace.filter((item) => item !== analysis)]) {
    for (const symbol of document.symbols) {
      if (symbol.name === name || symbol.name.endsWith(`::${name}`)) {
        matches.push(symbol);
      }
    }
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

export function includeAt(
  analysis: DocumentAnalysis,
  offset: number,
): IncludeReference | undefined {
  return analysis.includes.find((item) => offset >= item.start && offset <= item.end);
}

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

import type { DocumentAnalysis } from "./analyze";
import {
  TORQUE_ANNOTATIONS,
  TORQUE_BUILTINS,
  TORQUE_COMMON_TYPES,
  TORQUE_KEYWORD_DOCS,
  TORQUE_KEYWORDS,
  TORQUE_SNIPPETS,
} from "./keywords";
import type { Token } from "./lexer";

export type CompletionKind = "builtin" | "keyword" | "snippet" | "symbol" | "type";

export type TorqueCompletion = {
  label: string;
  kind: CompletionKind;
  insertText?: string;
  detail?: string;
  documentation?: string;
  sortText?: string;
};

function prefixAt(text: string, offset: number): { prefix: string; trigger: string | undefined } {
  const before = text.slice(0, offset);
  const at = /@[A-Za-z0-9_]*$/.exec(before);
  if (at !== null) {
    return { prefix: at[0], trigger: "@" };
  }
  const hash = /#[A-Za-z]*$/.exec(before);
  if (hash !== null) {
    return { prefix: hash[0], trigger: "#" };
  }
  const ident = /[A-Za-z_][A-Za-z0-9_-]*$/.exec(before);
  return { prefix: ident?.[0] ?? "", trigger: undefined };
}

function inNonCodeToken(token: Token | undefined): boolean {
  return token?.kind === "comment" || token?.kind === "string";
}

function matchesPrefix(label: string, prefix: string): boolean {
  if (prefix === "") {
    return true;
  }
  return label.toLowerCase().startsWith(prefix.toLowerCase());
}

export function completionsFor(
  analysis: DocumentAnalysis,
  offset: number,
  workspace: readonly DocumentAnalysis[],
): TorqueCompletion[] {
  const token = analysis.tokens.find((item) => offset > item.start && offset < item.end);
  if (inNonCodeToken(token)) {
    return [];
  }
  const { prefix, trigger } = prefixAt(analysis.text, offset);
  const items: TorqueCompletion[] = [];
  const seen = new Set<string>();
  const add = (item: TorqueCompletion): void => {
    if (seen.has(item.label)) {
      return;
    }
    seen.add(item.label);
    items.push(item);
  };

  if (trigger === "@" || prefix.startsWith("@")) {
    for (const annotation of TORQUE_ANNOTATIONS) {
      if (matchesPrefix(annotation, prefix)) {
        add({
          label: annotation,
          kind: "keyword",
          detail: "annotation",
          sortText: `1_${annotation}`,
        });
      }
    }
    return items;
  }

  if (trigger === "#" || prefix.startsWith("#")) {
    if (matchesPrefix("#include", prefix)) {
      add({
        label: "#include",
        kind: "snippet",
        insertText: '#include "${1:path}"',
        documentation: "Include a C++ header from Torque.",
        sortText: "0_include",
      });
    }
    return items;
  }

  for (const snippet of TORQUE_SNIPPETS) {
    if (matchesPrefix(snippet.label, prefix)) {
      add({
        label: snippet.label,
        kind: "snippet",
        insertText: snippet.insertText,
        documentation: snippet.documentation,
        sortText: `0_${snippet.label}`,
      });
    }
  }

  for (const keyword of TORQUE_KEYWORDS) {
    if (matchesPrefix(keyword, prefix)) {
      add({
        label: keyword,
        kind: "keyword",
        documentation: TORQUE_KEYWORD_DOCS[keyword],
        sortText: `1_${keyword}`,
      });
    }
  }

  for (const builtin of TORQUE_BUILTINS) {
    if (matchesPrefix(builtin.name, prefix)) {
      add({
        label: builtin.name,
        kind: "builtin",
        detail: builtin.detail,
        documentation: builtin.documentation,
        sortText: `2_${builtin.name}`,
      });
    }
  }

  for (const typeName of TORQUE_COMMON_TYPES) {
    if (matchesPrefix(typeName, prefix)) {
      add({
        label: typeName,
        kind: "type",
        detail: "type",
        sortText: `3_${typeName}`,
      });
    }
  }

  for (const document of [analysis, ...workspace]) {
    for (const symbol of document.symbols) {
      if (!matchesPrefix(symbol.name, prefix)) {
        continue;
      }
      add({
        label: symbol.name,
        kind: "symbol",
        detail: symbol.detail ?? symbol.kind,
        sortText: `4_${symbol.name}`,
      });
    }
  }

  return items;
}

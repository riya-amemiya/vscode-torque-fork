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

import { readFileSync } from "node:fs";
import {
  CompletionItem,
  CompletionItemKind,
  Diagnostic,
  DiagnosticSeverity,
  DocumentSymbol,
  EventEmitter,
  Hover,
  Location,
  MarkdownString,
  Position,
  Range,
  SemanticTokensBuilder,
  SemanticTokensLegend,
  SnippetString,
  SymbolKind,
  Uri,
  languages,
  workspace as vsWorkspace,
  type ExtensionContext,
  type TextDocument,
} from "vscode";
import {
  diagnosticRange,
  definitionRange,
  includeAt,
  resolveDefinition,
  symbolRange,
  type DocumentAnalysis,
  type TorqueSymbol,
  type TorqueSymbolKind,
} from "./language/analyze";
import { completionsFor, type CompletionKind } from "./language/complete";
import { hoverFor } from "./language/hover";
import { positionToOffset } from "./language/positions";
import {
  SEMANTIC_TOKEN_MODIFIERS,
  SEMANTIC_TOKEN_TYPES,
  semanticTokensFor,
} from "./language/semantic-tokens";
import { TorqueWorkspace } from "./language/workspace";
import { findTorqueRoot, listTorqueFiles } from "./language/workspace-files";

const SEMANTIC_LEGEND = new SemanticTokensLegend(
  [...SEMANTIC_TOKEN_TYPES],
  [...SEMANTIC_TOKEN_MODIFIERS],
);

const SELECTOR = { language: "torque", scheme: "file" };

function vscodeRange(range: {
  start: { line: number; character: number };
  end: { line: number; character: number };
}): Range {
  return new Range(
    new Position(range.start.line, range.start.character),
    new Position(range.end.line, range.end.character),
  );
}

function symbolKind(kind: TorqueSymbolKind): SymbolKind {
  switch (kind) {
    case "namespace":
      return SymbolKind.Namespace;
    case "type":
      return SymbolKind.TypeParameter;
    case "class":
      return SymbolKind.Class;
    case "struct":
      return SymbolKind.Struct;
    case "enum":
      return SymbolKind.Enum;
    case "macro":
    case "builtin":
    case "runtime":
    case "intrinsic":
      return SymbolKind.Function;
    case "const":
      return SymbolKind.Constant;
    case "let":
      return SymbolKind.Variable;
    case "label":
      return SymbolKind.Key;
    case "field":
      return SymbolKind.Field;
    case "shape":
      return SymbolKind.Object;
    default: {
      const exhaustive: never = kind;
      return exhaustive;
    }
  }
}

function completionKind(kind: CompletionKind): CompletionItemKind {
  switch (kind) {
    case "keyword":
      return CompletionItemKind.Keyword;
    case "snippet":
      return CompletionItemKind.Snippet;
    case "builtin":
      return CompletionItemKind.Function;
    case "type":
      return CompletionItemKind.TypeParameter;
    case "symbol":
      return CompletionItemKind.Reference;
    default: {
      const exhaustive: never = kind;
      return exhaustive;
    }
  }
}

function offsetOf(analysis: DocumentAnalysis, position: Position): number {
  return positionToOffset(analysis.lines, {
    line: position.line,
    character: position.character,
  });
}

function locationForDefinition(
  store: TorqueWorkspace,
  analysis: DocumentAnalysis,
  offset: number,
  fallback: Uri,
): Location[] {
  const hits = analysis.definitions.filter(
    (item) => offset >= item.fromStart && offset <= item.fromEnd,
  );
  if (hits.length === 0) {
    return [];
  }
  return hits.map((hit) => {
    const target = store.get(hit.toUri) ?? analysis;
    return new Location(
      Uri.parse(hit.toUri || fallback.toString()),
      vscodeRange(definitionRange(target, hit)),
    );
  });
}

function locationForSymbol(store: TorqueWorkspace, symbol: TorqueSymbol, fallback: Uri): Location {
  for (const [uri, analysis] of store.entries()) {
    if (analysis.symbols.includes(symbol)) {
      return new Location(Uri.parse(uri), vscodeRange(symbolRange(analysis, symbol)));
    }
  }
  return new Location(fallback, new Range(new Position(0, 0), new Position(0, 0)));
}

export function registerTorqueLanguage(context: ExtensionContext, store: TorqueWorkspace): void {
  const diagnostics = languages.createDiagnosticCollection("torque-compiler");
  const semanticChange = new EventEmitter<void>();

  const publish = (uri: Uri, analysis: DocumentAnalysis): void => {
    diagnostics.set(
      uri,
      analysis.diagnostics.map((item) => {
        const diagnostic = new Diagnostic(
          vscodeRange(diagnosticRange(analysis, item)),
          item.message,
          DiagnosticSeverity.Error,
        );
        diagnostic.source = "Torque Compiler";
        return diagnostic;
      }),
    );
  };

  const publishAll = (): void => {
    for (const [uri, analysis] of store.entries()) {
      publish(Uri.parse(uri), analysis);
    }
  };

  const rebuildAndPublish = (): void => {
    store.rebuild();
    publishAll();
    semanticChange.fire();
  };

  const disk = { loaded: false };

  const loadDiskFiles = (seedPath: string): void => {
    if (disk.loaded) {
      return;
    }
    const root = findTorqueRoot(seedPath);
    if (root === undefined) {
      return;
    }
    disk.loaded = true;
    for (const fsPath of listTorqueFiles(root)) {
      const uri = Uri.file(fsPath).toString();
      if (store.hasSource(uri)) {
        continue;
      }
      try {
        store.load(uri, readFileSync(fsPath, "utf8"));
      } catch {
        continue;
      }
    }
  };

  const refreshDocument = (document: TextDocument): void => {
    if (document.languageId !== "torque") {
      return;
    }
    const uri = document.uri.toString();
    const text = document.getText();
    if (store.get(uri)?.text === text) {
      return;
    }
    store.load(uri, text);
    publish(document.uri, store.refresh(uri));
    semanticChange.fire();
  };

  const ingestOpen = (document: TextDocument): void => {
    if (document.languageId !== "torque") {
      return;
    }
    const uri = document.uri.toString();
    store.load(uri, document.getText());
    if (document.uri.scheme === "file") {
      loadDiskFiles(document.uri.fsPath);
    }
    if (store.get(uri) !== undefined) {
      refreshDocument(document);
      return;
    }
    rebuildAndPublish();
  };

  for (const document of vsWorkspace.textDocuments) {
    ingestOpen(document);
  }

  void vsWorkspace
    .findFiles("**/*.tq", "{**/out/**,**/node_modules/**,**/build/**}")
    .then(async (files) => {
      for (const file of files) {
        if (store.hasSource(file.toString())) {
          continue;
        }
        const bytes = await vsWorkspace.fs.readFile(file);
        store.load(file.toString(), Buffer.from(bytes).toString("utf8"));
      }
      rebuildAndPublish();
    });

  context.subscriptions.push(
    diagnostics,
    semanticChange,
    vsWorkspace.onDidOpenTextDocument(ingestOpen),
    vsWorkspace.onDidChangeTextDocument((event) => {
      if (event.contentChanges.length === 0) {
        return;
      }
      refreshDocument(event.document);
    }),
    vsWorkspace.onDidSaveTextDocument((document) => {
      refreshDocument(document);
    }),
    vsWorkspace.onDidCloseTextDocument((document) => {
      diagnostics.delete(document.uri);
    }),
    languages.registerCompletionItemProvider(
      SELECTOR,
      {
        provideCompletionItems(document, position) {
          const analysis = store.ensure(document.uri.toString(), document.getText());
          return completionsFor(analysis, offsetOf(analysis, position), store.all()).map((item) => {
            const completion = new CompletionItem(item.label, completionKind(item.kind));
            completion.detail = item.detail;
            completion.documentation = item.documentation;
            completion.sortText = item.sortText;
            if (item.insertText !== undefined) {
              completion.insertText = new SnippetString(item.insertText);
            }
            return completion;
          });
        },
      },
      "@",
      "#",
    ),
    languages.registerDefinitionProvider(SELECTOR, {
      async provideDefinition(document, position) {
        const analysis = store.ensure(document.uri.toString(), document.getText());
        const offset = offsetOf(analysis, position);
        const include = includeAt(analysis, offset);
        if (include !== undefined) {
          const matches = await vsWorkspace.findFiles(`**/${include.path}`, undefined, 5);
          if (matches.length > 0) {
            return matches.map((uri) => new Location(uri, new Position(0, 0)));
          }
          const folder = vsWorkspace.workspaceFolders?.[0];
          if (folder !== undefined) {
            return new Location(Uri.joinPath(folder.uri, include.path), new Position(0, 0));
          }
        }
        const mapped = locationForDefinition(store, analysis, offset, document.uri);
        if (mapped.length > 0) {
          return mapped;
        }
        const symbols = resolveDefinition(analysis, offset, store.all());
        if (symbols.length === 0) {
          return undefined;
        }
        return symbols.map((symbol) => locationForSymbol(store, symbol, document.uri));
      },
    }),
    languages.registerHoverProvider(SELECTOR, {
      provideHover(document, position) {
        const analysis = store.ensure(document.uri.toString(), document.getText());
        const hover = hoverFor(analysis, offsetOf(analysis, position));
        if (hover === undefined) {
          return undefined;
        }
        const markdown = new MarkdownString();
        markdown.appendCodeblock(hover.title, "torque");
        markdown.appendMarkdown(hover.body);
        return new Hover(markdown);
      },
    }),
    languages.registerDocumentSymbolProvider(SELECTOR, {
      provideDocumentSymbols(document) {
        const analysis = store.ensure(document.uri.toString(), document.getText());
        return analysis.symbols.map((symbol) => {
          const range = vscodeRange(symbolRange(analysis, symbol));
          return new DocumentSymbol(
            symbol.name,
            symbol.detail ?? symbol.kind,
            symbolKind(symbol.kind),
            range,
            range,
          );
        });
      },
    }),
    languages.registerDocumentSemanticTokensProvider(
      SELECTOR,
      {
        onDidChangeSemanticTokens: semanticChange.event,
        provideDocumentSemanticTokens(document) {
          const analysis = store.ensure(document.uri.toString(), document.getText());
          const builder = new SemanticTokensBuilder(SEMANTIC_LEGEND);
          for (const span of semanticTokensFor(analysis, store.all())) {
            builder.push(
              span.line,
              span.character,
              span.length,
              SEMANTIC_TOKEN_TYPES.indexOf(span.type),
              span.modifiers.reduce(
                (bits, modifier) => bits | (1 << SEMANTIC_TOKEN_MODIFIERS.indexOf(modifier)),
                0,
              ),
            );
          }
          return builder.build();
        },
      },
      SEMANTIC_LEGEND,
    ),
  );
}

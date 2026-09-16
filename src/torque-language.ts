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

import {
  CompletionItem,
  CompletionItemKind,
  Diagnostic,
  DiagnosticSeverity,
  DocumentSymbol,
  Hover,
  Location,
  MarkdownString,
  Position,
  Range,
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
import { TorqueWorkspace } from "./language/workspace";

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

function locationForSymbol(store: TorqueWorkspace, symbol: TorqueSymbol, fallback: Uri): Location {
  for (const [uri, analysis] of store.entries()) {
    if (analysis.symbols.includes(symbol)) {
      return new Location(Uri.parse(uri), vscodeRange(symbolRange(analysis, symbol)));
    }
  }
  return new Location(fallback, new Range(new Position(0, 0), new Position(0, 0)));
}

export function registerTorqueLanguage(context: ExtensionContext, store: TorqueWorkspace): void {
  const diagnostics = languages.createDiagnosticCollection("torque-syntax");

  const publish = (uri: Uri, analysis: DocumentAnalysis): void => {
    diagnostics.set(
      uri,
      analysis.diagnostics.map((item) => {
        const diagnostic = new Diagnostic(
          vscodeRange(diagnosticRange(analysis, item)),
          item.message,
          DiagnosticSeverity.Error,
        );
        diagnostic.source = "Torque";
        return diagnostic;
      }),
    );
  };

  const ingest = (document: TextDocument): void => {
    if (document.languageId !== "torque") {
      return;
    }
    publish(document.uri, store.set(document.uri.toString(), document.getText()));
  };

  for (const document of vsWorkspace.textDocuments) {
    ingest(document);
  }

  void vsWorkspace.findFiles("**/*.tq").then(async (files) => {
    for (const file of files) {
      if (store.get(file.toString()) !== undefined) {
        continue;
      }
      const bytes = await vsWorkspace.fs.readFile(file);
      store.set(file.toString(), Buffer.from(bytes).toString("utf8"));
    }
  });

  context.subscriptions.push(
    diagnostics,
    vsWorkspace.onDidOpenTextDocument(ingest),
    vsWorkspace.onDidChangeTextDocument((event) => ingest(event.document)),
    vsWorkspace.onDidCloseTextDocument((document) => {
      diagnostics.delete(document.uri);
    }),
    languages.registerCompletionItemProvider(
      SELECTOR,
      {
        provideCompletionItems(document, position) {
          const analysis =
            store.get(document.uri.toString()) ??
            store.set(document.uri.toString(), document.getText());
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
        const analysis =
          store.get(document.uri.toString()) ??
          store.set(document.uri.toString(), document.getText());
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
        const symbols = resolveDefinition(analysis, offset, store.all());
        if (symbols.length === 0) {
          return undefined;
        }
        return symbols.map((symbol) => locationForSymbol(store, symbol, document.uri));
      },
    }),
    languages.registerHoverProvider(SELECTOR, {
      provideHover(document, position) {
        const analysis =
          store.get(document.uri.toString()) ??
          store.set(document.uri.toString(), document.getText());
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
        const analysis =
          store.get(document.uri.toString()) ??
          store.set(document.uri.toString(), document.getText());
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
  );
}

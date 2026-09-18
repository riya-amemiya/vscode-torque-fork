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
  analyzeDocument,
  analyzeDocuments,
  dropResolvedElsewhere,
  type DocumentAnalysis,
} from "./analyze";

export class TorqueWorkspace {
  private readonly sources = new Map<string, string>();
  private readonly documents = new Map<string, DocumentAnalysis>();
  private dirty = false;

  hasSource(uri: string): boolean {
    return this.sources.has(uri);
  }

  set(uri: string, text: string): DocumentAnalysis {
    this.sources.set(uri, text);
    this.dirty = true;
    this.rebuild();
    const analysis = this.documents.get(uri);
    if (analysis === undefined) {
      throw new Error(`Torque compiler did not return analysis for ${uri}`);
    }
    return analysis;
  }

  load(uri: string, text: string): void {
    const previous = this.sources.get(uri);
    if (previous === text && this.documents.has(uri) && !this.dirty) {
      return;
    }
    this.sources.set(uri, text);
    this.dirty = true;
  }

  isDirty(): boolean {
    return this.dirty;
  }

  ensure(uri: string, text: string): DocumentAnalysis {
    const hadAnalysis = this.documents.has(uri);
    this.load(uri, text);
    if (!hadAnalysis) {
      this.rebuild();
    }
    const analysis = this.documents.get(uri);
    if (analysis === undefined) {
      throw new Error(`Torque compiler did not return analysis for ${uri}`);
    }
    return analysis;
  }

  refresh(uri: string): DocumentAnalysis {
    const text = this.sources.get(uri);
    if (text === undefined) {
      throw new Error(`Torque workspace has no source for ${uri}`);
    }
    const current = this.documents.get(uri);
    if (current !== undefined && current.text === text) {
      return current;
    }
    const siblings = [...this.documents.entries()]
      .filter(([itemUri]) => itemUri !== uri)
      .map(([, document]) => document);
    const analysis = dropResolvedElsewhere(analyzeDocument(text, uri), siblings);
    this.documents.set(uri, analysis);
    return analysis;
  }

  rebuild(): void {
    const files = [...this.sources.entries()].map(([uri, text]) => ({ uri, text }));
    try {
      const compiled = analyzeDocuments(files);
      this.documents.clear();
      for (const [uri, analysis] of compiled) {
        this.documents.set(uri, analysis);
      }
      this.fillMissing(files);
    } catch {
      this.rebuildEach(files);
    }
    this.dirty = false;
  }

  private fillMissing(files: Array<{ uri: string; text: string }>): void {
    for (const file of files) {
      if (this.documents.has(file.uri)) {
        continue;
      }
      this.documents.set(file.uri, analyzeDocument(file.text, file.uri));
    }
  }

  private rebuildEach(files: Array<{ uri: string; text: string }>): void {
    this.documents.clear();
    for (const file of files) {
      try {
        this.documents.set(file.uri, analyzeDocument(file.text, file.uri));
      } catch {
        continue;
      }
    }
  }

  delete(uri: string): void {
    this.sources.delete(uri);
    this.rebuild();
  }

  get(uri: string): DocumentAnalysis | undefined {
    return this.documents.get(uri);
  }

  uriFor(analysis: DocumentAnalysis): string | undefined {
    return analysis.uri;
  }

  all(): DocumentAnalysis[] {
    return [...this.documents.values()];
  }

  entries(): IterableIterator<[string, DocumentAnalysis]> {
    return this.documents.entries();
  }
}

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

import { analyzeDocuments, type DocumentAnalysis } from "./analyze";

export class TorqueWorkspace {
  private readonly sources = new Map<string, string>();
  private readonly documents = new Map<string, DocumentAnalysis>();

  set(uri: string, text: string): DocumentAnalysis {
    this.sources.set(uri, text);
    this.rebuild();
    const analysis = this.documents.get(uri);
    if (analysis === undefined) {
      throw new Error(`Torque compiler did not return analysis for ${uri}`);
    }
    return analysis;
  }

  load(uri: string, text: string): void {
    this.sources.set(uri, text);
  }

  rebuild(): void {
    const files = [...this.sources.entries()].map(([uri, text]) => ({ uri, text }));
    const compiled = analyzeDocuments(files);
    this.documents.clear();
    for (const [uri, analysis] of compiled) {
      this.documents.set(uri, analysis);
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

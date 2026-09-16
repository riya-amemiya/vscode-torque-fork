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

import { analyzeDocument, type DocumentAnalysis } from "./analyze";

export class TorqueWorkspace {
  private readonly documents = new Map<string, DocumentAnalysis>();

  set(uri: string, text: string): DocumentAnalysis {
    const analysis = analyzeDocument(text);
    this.documents.set(uri, analysis);
    return analysis;
  }

  delete(uri: string): void {
    this.documents.delete(uri);
  }

  get(uri: string): DocumentAnalysis | undefined {
    return this.documents.get(uri);
  }

  uriFor(analysis: DocumentAnalysis): string | undefined {
    for (const [uri, document] of this.documents) {
      if (document === analysis) {
        return uri;
      }
    }
    return undefined;
  }

  all(): DocumentAnalysis[] {
    return [...this.documents.values()];
  }

  entries(): IterableIterator<[string, DocumentAnalysis]> {
    return this.documents.entries();
  }
}

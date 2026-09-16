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

import { identifierAt, type DocumentAnalysis } from "./analyze";
import { TORQUE_BUILTINS, TORQUE_KEYWORD_DOCS } from "./keywords";

export type TorqueHover = {
  title: string;
  body: string;
};

export function hoverFor(analysis: DocumentAnalysis, offset: number): TorqueHover | undefined {
  const token = identifierAt(analysis, offset);
  if (token === undefined) {
    return undefined;
  }
  if (token.kind === "keyword") {
    const body = TORQUE_KEYWORD_DOCS[token.text];
    if (body === undefined) {
      return { title: token.text, body: "Torque keyword" };
    }
    return { title: token.text, body };
  }
  const builtin = TORQUE_BUILTINS.find((item) => item.name === token.text);
  if (builtin !== undefined) {
    return { title: builtin.detail, body: builtin.documentation };
  }
  const symbol = analysis.symbols.find((item) => offset >= item.start && offset <= item.end);
  const named = analysis.symbols.find((item) => item.name === token.text);
  const match = symbol ?? named;
  if (match !== undefined) {
    const qualifier = match.containerName === undefined ? "" : `${match.containerName}::`;
    return {
      title: `${match.kind} ${qualifier}${match.name}`,
      body: match.detail ?? "Torque declaration",
    };
  }
  return undefined;
}

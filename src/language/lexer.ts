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

import { TORQUE_KEYWORD_SET } from "./keywords";

export type TokenKind =
  | "annotation"
  | "comment"
  | "error"
  | "identifier"
  | "include"
  | "keyword"
  | "number"
  | "punct"
  | "string";

export type Token = {
  kind: TokenKind;
  text: string;
  start: number;
  end: number;
  message?: string;
};

const MULTI_PUNCT = [
  "...",
  "::",
  "=>",
  "==",
  "!=",
  "<=",
  ">=",
  "&&",
  "||",
  "++",
  "--",
  "->",
  "+=",
  "-=",
  "*=",
  "/=",
];

function isIdentStart(code: number): boolean {
  return (code >= 65 && code <= 90) || (code >= 97 && code <= 122) || code === 95;
}

function isIdentPart(code: number): boolean {
  return isIdentStart(code) || (code >= 48 && code <= 57);
}

function isDigit(code: number): boolean {
  return code >= 48 && code <= 57;
}

function isHex(code: number): boolean {
  return isDigit(code) || (code >= 65 && code <= 70) || (code >= 97 && code <= 102);
}

export function tokenize(text: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;

  const push = (token: Token): void => {
    tokens.push(token);
  };

  while (index < text.length) {
    const start = index;
    const code = text.charCodeAt(index);

    if (code === 32 || code === 9 || code === 10 || code === 13) {
      index += 1;
      continue;
    }

    if (code === 47 && text.charCodeAt(index + 1) === 47) {
      index += 2;
      while (index < text.length && text.charCodeAt(index) !== 10) {
        index += 1;
      }
      push({ kind: "comment", text: text.slice(start, index), start, end: index });
      continue;
    }

    if (code === 47 && text.charCodeAt(index + 1) === 42) {
      index += 2;
      let terminated = false;
      while (index < text.length) {
        if (text.charCodeAt(index) === 42 && text.charCodeAt(index + 1) === 47) {
          index += 2;
          terminated = true;
          break;
        }
        index += 1;
      }
      push(
        terminated
          ? { kind: "comment", text: text.slice(start, index), start, end: index }
          : {
              kind: "error",
              text: text.slice(start, index),
              start,
              end: index,
              message: "Unterminated block comment",
            },
      );
      continue;
    }

    if (code === 34 || code === 39) {
      const quote = code;
      index += 1;
      let terminated = false;
      while (index < text.length) {
        const current = text.charCodeAt(index);
        if (current === 92) {
          index += 2;
          continue;
        }
        if (current === quote) {
          index += 1;
          terminated = true;
          break;
        }
        if (current === 10) {
          break;
        }
        index += 1;
      }
      push(
        terminated
          ? { kind: "string", text: text.slice(start, index), start, end: index }
          : {
              kind: "error",
              text: text.slice(start, index),
              start,
              end: Math.max(index, start + 1),
              message: "Unterminated string literal",
            },
      );
      continue;
    }

    if (code === 35) {
      if (text.startsWith("#include", index)) {
        index += "#include".length;
        push({ kind: "include", text: "#include", start, end: index });
        continue;
      }
      index += 1;
      while (index < text.length && isIdentPart(text.charCodeAt(index))) {
        index += 1;
      }
      push({
        kind: "error",
        text: text.slice(start, index),
        start,
        end: index,
        message: "Unknown preprocessor directive",
      });
      continue;
    }

    if (code === 64) {
      index += 1;
      while (index < text.length && isIdentPart(text.charCodeAt(index))) {
        index += 1;
      }
      if (index === start + 1) {
        push({
          kind: "error",
          text: "@",
          start,
          end: index,
          message: "Expected annotation name after '@'",
        });
        continue;
      }
      push({ kind: "annotation", text: text.slice(start, index), start, end: index });
      continue;
    }

    if (isDigit(code)) {
      if (
        code === 48 &&
        (text.charCodeAt(index + 1) === 120 || text.charCodeAt(index + 1) === 88)
      ) {
        index += 2;
        while (index < text.length && isHex(text.charCodeAt(index))) {
          index += 1;
        }
      } else {
        while (index < text.length && isDigit(text.charCodeAt(index))) {
          index += 1;
        }
        if (text.charCodeAt(index) === 46 && isDigit(text.charCodeAt(index + 1))) {
          index += 1;
          while (index < text.length && isDigit(text.charCodeAt(index))) {
            index += 1;
          }
        }
      }
      push({ kind: "number", text: text.slice(start, index), start, end: index });
      continue;
    }

    if (isIdentStart(code)) {
      if (text.startsWith("js-implicit", index)) {
        const after = text.charCodeAt(index + "js-implicit".length);
        if (!after || !isIdentPart(after)) {
          index += "js-implicit".length;
          push({ kind: "keyword", text: "js-implicit", start, end: index });
          continue;
        }
      }
      index += 1;
      while (index < text.length && isIdentPart(text.charCodeAt(index))) {
        index += 1;
      }
      const value = text.slice(start, index);
      push({
        kind: TORQUE_KEYWORD_SET.has(value) ? "keyword" : "identifier",
        text: value,
        start,
        end: index,
      });
      continue;
    }

    const rest = text.slice(index);
    const multi = MULTI_PUNCT.find((item) => rest.startsWith(item));
    if (multi !== undefined) {
      index += multi.length;
      push({ kind: "punct", text: multi, start, end: index });
      continue;
    }

    index += 1;
    const punct = text.slice(start, index);
    const allowed = "{}[]()<>:;,.?=+-*/%|&!~^".includes(punct) || punct === "\\";
    if (allowed) {
      push({ kind: "punct", text: punct, start, end: index });
    } else {
      push({
        kind: "error",
        text: punct,
        start,
        end: index,
        message: `Unexpected character '${punct}'`,
      });
    }
  }

  return tokens;
}

export function tokenAt(tokens: Token[], offset: number): Token | undefined {
  for (const token of tokens) {
    if (offset >= token.start && offset <= token.end) {
      if (offset === token.end && token.end !== token.start) {
        continue;
      }
      return token;
    }
  }
  for (let index = tokens.length - 1; index >= 0; index -= 1) {
    const token = tokens[index];
    if (token.start <= offset && offset <= token.end) {
      return token;
    }
  }
  return undefined;
}

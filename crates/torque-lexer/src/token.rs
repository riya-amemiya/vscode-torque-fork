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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Annotation,
    Comment,
    Error,
    Identifier,
    Include,
    Import,
    Keyword,
    Number,
    Punct,
    String,
    Intrinsic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub message: Option<String>,
}

pub const KEYWORDS: &[&str] = &[
    "bitfield",
    "break",
    "builtin",
    "case",
    "catch",
    "class",
    "const",
    "constexpr",
    "continue",
    "deferred",
    "else",
    "enum",
    "extends",
    "extern",
    "for",
    "generates",
    "goto",
    "if",
    "implicit",
    "import",
    "intrinsic",
    "javascript",
    "js-implicit",
    "label",
    "labels",
    "let",
    "macro",
    "namespace",
    "never",
    "new",
    "operator",
    "otherwise",
    "return",
    "runtime",
    "shape",
    "struct",
    "tail",
    "transient",
    "transitioning",
    "try",
    "type",
    "typeswitch",
    "void",
    "weak",
    "while",
];

pub fn is_keyword(text: &str) -> bool {
    KEYWORDS.contains(&text)
}

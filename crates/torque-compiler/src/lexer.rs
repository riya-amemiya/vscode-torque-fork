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

use crate::token::{Token, TokenKind, is_keyword};

const MULTI_PUNCT: &[&str] = &[
    "...", "::", "=>", "==", "!=", "<=", ">=", "&&", "||", "++", "--", "->", "+=", "-=", "*=",
    "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", ">>>=", ">>>", "<<", ">>",
];

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_part(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

pub fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        let start = index;
        let c = bytes[index];

        if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
            index += 1;
            continue;
        }

        if c == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: text[start..index].to_string(),
                start,
                end: index,
                message: None,
            });
            continue;
        }

        if c == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            let mut terminated = false;
            while index < bytes.len() {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    index += 2;
                    terminated = true;
                    break;
                }
                index += 1;
            }
            tokens.push(if terminated {
                Token {
                    kind: TokenKind::Comment,
                    text: text[start..index].to_string(),
                    start,
                    end: index,
                    message: None,
                }
            } else {
                Token {
                    kind: TokenKind::Error,
                    text: text[start..index].to_string(),
                    start,
                    end: index,
                    message: Some("Unterminated block comment".into()),
                }
            });
            continue;
        }

        if c == b'"' || c == b'\'' {
            let quote = c;
            index += 1;
            let mut terminated = false;
            while index < bytes.len() {
                let current = bytes[index];
                if current == b'\\' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if current == quote {
                    index += 1;
                    terminated = true;
                    break;
                }
                if current == b'\n' {
                    break;
                }
                index += 1;
            }
            tokens.push(if terminated {
                Token {
                    kind: TokenKind::String,
                    text: text[start..index].to_string(),
                    start,
                    end: index,
                    message: None,
                }
            } else {
                Token {
                    kind: TokenKind::Error,
                    text: text[start..index].to_string(),
                    start,
                    end: start.max(index).max(start + 1),
                    message: Some("Unterminated string literal".into()),
                }
            });
            continue;
        }

        if c == b'#' {
            if text[index..].starts_with("#include") {
                index += "#include".len();
                tokens.push(Token {
                    kind: TokenKind::Include,
                    text: "#include".into(),
                    start,
                    end: index,
                    message: None,
                });
                continue;
            }
            index += 1;
            while index < bytes.len() && is_ident_part(bytes[index]) {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Error,
                text: text[start..index].to_string(),
                start,
                end: index,
                message: Some("Unknown preprocessor directive".into()),
            });
            continue;
        }

        if c == b'%' && bytes.get(index + 1).is_some_and(|n| is_ident_start(*n)) {
            index += 1;
            while index < bytes.len() && is_ident_part(bytes[index]) {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Intrinsic,
                text: text[start..index].to_string(),
                start,
                end: index,
                message: None,
            });
            continue;
        }

        if c == b'@' {
            index += 1;
            while index < bytes.len() && is_ident_part(bytes[index]) {
                index += 1;
            }
            if index == start + 1 {
                tokens.push(Token {
                    kind: TokenKind::Error,
                    text: "@".into(),
                    start,
                    end: index,
                    message: Some("Expected annotation name after '@'".into()),
                });
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Annotation,
                text: text[start..index].to_string(),
                start,
                end: index,
                message: None,
            });
            continue;
        }

        if c.is_ascii_digit() {
            if c == b'0' && matches!(bytes.get(index + 1), Some(&b'x' | &b'X')) {
                index += 2;
                while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
                    index += 1;
                }
            } else {
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
                if bytes.get(index) == Some(&b'.')
                    && bytes.get(index + 1).is_some_and(|n| n.is_ascii_digit())
                {
                    index += 1;
                    while index < bytes.len() && bytes[index].is_ascii_digit() {
                        index += 1;
                    }
                    if matches!(bytes.get(index), Some(&b'e' | &b'E')) {
                        let mut exp = index + 1;
                        if matches!(bytes.get(exp), Some(&b'+' | &b'-')) {
                            exp += 1;
                        }
                        if bytes.get(exp).is_some_and(|n| n.is_ascii_digit()) {
                            index = exp;
                            while index < bytes.len() && bytes[index].is_ascii_digit() {
                                index += 1;
                            }
                        }
                    }
                }
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: text[start..index].to_string(),
                start,
                end: index,
                message: None,
            });
            continue;
        }

        if is_ident_start(c) {
            if text[index..].starts_with("js-implicit") {
                let after = bytes.get(index + "js-implicit".len()).copied().unwrap_or(0);
                if !is_ident_part(after) {
                    index += "js-implicit".len();
                    tokens.push(Token {
                        kind: TokenKind::Keyword,
                        text: "js-implicit".into(),
                        start,
                        end: index,
                        message: None,
                    });
                    continue;
                }
            }
            index += 1;
            while index < bytes.len() && is_ident_part(bytes[index]) {
                index += 1;
            }
            let value = &text[start..index];
            let kind = if value == "import" {
                TokenKind::Import
            } else if is_keyword(value) {
                TokenKind::Keyword
            } else {
                TokenKind::Identifier
            };
            tokens.push(Token {
                kind,
                text: value.to_string(),
                start,
                end: index,
                message: None,
            });
            continue;
        }

        let rest = &text[index..];
        if let Some(multi) = MULTI_PUNCT.iter().find(|item| rest.starts_with(*item)) {
            index += multi.len();
            tokens.push(Token {
                kind: TokenKind::Punct,
                text: (*multi).to_string(),
                start,
                end: index,
                message: None,
            });
            continue;
        }

        index += 1;
        let punct = &text[start..index];
        let allowed = "{}[]()<>:;,.?=+-*/%|&!~^".contains(punct) || punct == "\\";
        tokens.push(if allowed {
            Token {
                kind: TokenKind::Punct,
                text: punct.to_string(),
                start,
                end: index,
                message: None,
            }
        } else {
            Token {
                kind: TokenKind::Error,
                text: punct.to_string(),
                start,
                end: index,
                message: Some(format!("Unexpected character '{punct}'")),
            }
        });
    }

    tokens
}

pub fn delimiter_errors(tokens: &[Token]) -> Vec<(usize, usize, String)> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<&Token> = Vec::new();
    for token in tokens {
        if token.kind != TokenKind::Punct {
            continue;
        }
        let closer = match token.text.as_str() {
            "(" => Some(")"),
            "[" => Some("]"),
            "{" => Some("}"),
            _ => None,
        };
        if closer.is_some() {
            stack.push(token);
            continue;
        }
        if matches!(token.text.as_str(), ")" | "]" | "}") {
            match stack.last() {
                None => diagnostics.push((
                    token.start,
                    token.end,
                    format!("Unmatched '{}'", token.text),
                )),
                Some(open) => {
                    let expected = match open.text.as_str() {
                        "(" => ")",
                        "[" => "]",
                        "{" => "}",
                        _ => "",
                    };
                    if expected != token.text {
                        diagnostics.push((
                            token.start,
                            token.end,
                            format!("Expected '{expected}' but found '{}'", token.text),
                        ));
                    }
                    stack.pop();
                }
            }
        }
    }
    for open in stack {
        diagnostics.push((open.start, open.end, format!("Unclosed '{}'", open.text)));
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_keywords_and_js_implicit() {
        let tokens = tokenize("javascript builtin Foo(js-implicit context: NativeContext)(): void");
        let kinds: Vec<String> = tokens
            .iter()
            .map(|token| {
                format!(
                    "{}:{}",
                    format!("{:?}", token.kind).to_lowercase(),
                    token.text
                )
            })
            .collect();
        assert!(kinds.iter().any(|k| k.contains("javascript")));
        assert!(
            kinds
                .iter()
                .any(|k| k == "keyword:builtin" || k.ends_with("builtin"))
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "Foo" && t.kind == TokenKind::Identifier)
        );
        assert!(tokens.iter().any(|t| t.text == "js-implicit"));
        assert!(tokens.iter().any(|t| t.text == "void"));
    }

    #[test]
    fn reports_unterminated_strings_and_comments() {
        assert!(
            tokenize("\"hello")
                .iter()
                .any(|t| t.kind == TokenKind::Error)
        );
        assert!(
            tokenize("/* oops")
                .iter()
                .any(|t| t.message.as_deref() == Some("Unterminated block comment"))
        );
    }

    #[test]
    fn keeps_include_directive() {
        let tokens = tokenize("#include \"src/objects/js-proxy.h\"");
        assert_eq!(tokens[0].kind, TokenKind::Include);
        assert_eq!(tokens[1].kind, TokenKind::String);
    }
}

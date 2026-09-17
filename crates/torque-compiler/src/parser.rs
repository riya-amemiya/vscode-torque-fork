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

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexer::{delimiter_errors, tokenize};
use crate::span::Span;
use crate::token::{Token, TokenKind};

pub struct ParseOutput {
    pub file: ParsedFile,
    pub diagnostics: Vec<Diagnostic>,
}

struct Parser<'a> {
    file: u32,
    tokens: &'a [Token],
    index: usize,
    extra_gt: u32,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn span_of(&self, token: &Token) -> Span {
        Span::new(self.file, token.start, token.end)
    }

    fn peek(&mut self) -> Option<&'a Token> {
        while self.index < self.tokens.len() && self.tokens[self.index].kind == TokenKind::Comment {
            self.index += 1;
        }
        self.tokens.get(self.index)
    }

    fn peek_kind(&mut self) -> Option<(&'a str, TokenKind)> {
        self.peek().map(|t| (t.text.as_str(), t.kind))
    }

    fn at(&mut self, text: &str) -> bool {
        if text == ">" {
            return self.at_gt();
        }
        self.peek().is_some_and(|t| t.text == text)
    }

    fn at_kind(&mut self, kind: TokenKind) -> bool {
        self.peek().is_some_and(|t| t.kind == kind)
    }

    fn take(&mut self) -> Option<&'a Token> {
        let token = self.peek()?;
        self.index += 1;
        Some(token)
    }

    fn eat(&mut self, text: &str) -> bool {
        if text == ">" {
            return self.eat_gt();
        }
        if self.at(text) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn at_gt(&mut self) -> bool {
        self.extra_gt > 0
            || matches!(
                self.peek().map(|t| t.text.as_str()),
                Some(">" | ">>" | ">>>")
            )
    }

    fn eat_gt(&mut self) -> bool {
        if self.extra_gt > 0 {
            self.extra_gt -= 1;
            return true;
        }
        match self.peek().map(|t| t.text.as_str()) {
            Some(">") => {
                self.index += 1;
                true
            }
            Some(">>") => {
                self.index += 1;
                self.extra_gt = 1;
                true
            }
            Some(">>>") => {
                self.index += 1;
                self.extra_gt = 2;
                true
            }
            _ => false,
        }
    }

    fn comma_separated<T>(&mut self, mut parse_item: impl FnMut(&mut Self) -> Option<T>) -> Vec<T> {
        let mut items = Vec::new();
        while let Some(item) = parse_item(self) {
            items.push(item);
            if !self.eat(",") {
                break;
            }
        }
        items
    }

    fn expect(&mut self, text: &str) -> Option<&'a Token> {
        if text == ">" {
            if self.eat_gt() {
                return self.tokens.get(self.index.saturating_sub(1));
            }
            let span = self
                .peek()
                .map(|t| self.span_of(t))
                .unwrap_or(Span::new(self.file, 0, 0));
            self.diagnostics
                .push(Diagnostic::error(span, "Expected '>'"));
            return None;
        }
        if self.at(text) {
            self.take()
        } else {
            let span = self
                .peek()
                .map(|t| self.span_of(t))
                .unwrap_or(Span::new(self.file, 0, 0));
            self.diagnostics
                .push(Diagnostic::error(span, format!("Expected '{text}'")));
            None
        }
    }

    fn error_here(&mut self, message: impl Into<String>) {
        let span = self
            .peek()
            .map(|t| self.span_of(t))
            .unwrap_or(Span::new(self.file, 0, 0));
        self.diagnostics.push(Diagnostic::error(span, message));
    }

    fn ident_from(&self, token: &Token) -> Ident {
        Ident {
            name: token.text.clone(),
            span: self.span_of(token),
        }
    }

    fn parse_name(&mut self) -> Option<Ident> {
        let token = self.peek()?;
        if matches!(
            token.kind,
            TokenKind::Identifier | TokenKind::Keyword | TokenKind::Intrinsic
        ) {
            let ident = self.ident_from(token);
            self.index += 1;
            return Some(ident);
        }
        None
    }

    fn parse_string_value(&mut self) -> Option<(String, Span)> {
        let token = self.peek()?;
        if token.kind != TokenKind::String {
            return None;
        }
        let span = self.span_of(token);
        let raw = token.text.clone();
        self.index += 1;
        let inner = if raw.len() >= 2 {
            raw[1..raw.len() - 1].to_string()
        } else {
            String::new()
        };
        Some((inner, span))
    }

    fn parse_annotations(&mut self) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        while self.at_kind(TokenKind::Annotation) {
            let token = self.take().unwrap();
            let mut annotation = Annotation {
                name: token.text.clone(),
                argument: None,
                span: self.span_of(token),
            };
            if self.eat("(") {
                let mut depth = 1;
                let mut parts = Vec::new();
                while depth > 0 {
                    let Some(token) = self.peek() else {
                        break;
                    };
                    if token.text == "(" {
                        depth += 1;
                    } else if token.text == ")" {
                        depth -= 1;
                        if depth == 0 {
                            annotation.span = annotation.span.merge(self.span_of(token));
                            self.take();
                            break;
                        }
                    }
                    let token = self.take().unwrap();
                    parts.push(token.text.clone());
                    annotation.span = annotation.span.merge(self.span_of(token));
                }
                if !parts.is_empty() {
                    annotation.argument = Some(parts.join(""));
                }
            }
            annotations.push(annotation);
        }
        annotations
    }

    fn parse_generic_args(&mut self) -> Vec<TypeExpr> {
        if !self.eat("<") {
            return Vec::new();
        }
        let args = if self.at(">") {
            Vec::new()
        } else {
            self.comma_separated(Self::parse_type)
        };
        self.expect(">");
        args
    }

    fn parse_generic_params(&mut self) -> Vec<GenericParam> {
        if !self.eat("<") {
            return Vec::new();
        }
        let params = if self.at(">") {
            Vec::new()
        } else {
            self.comma_separated(|parser| {
                let ty = parser.parse_type()?;
                let mut is_variable = false;
                if parser.eat(":") {
                    parser.eat("type");
                    is_variable = true;
                }
                if parser.eat("extends") {
                    let _ = parser.parse_type();
                    is_variable = true;
                }
                Some(GenericParam {
                    name: type_expr_as_ident(&ty),
                    is_variable,
                })
            })
        };
        self.expect(">");
        params
    }

    fn parse_namespace_and_name(&mut self) -> (Vec<String>, Ident) {
        let mut namespace = Vec::new();
        if self.eat("::") {
            namespace.push(String::new());
        }
        let first = self.parse_name().unwrap_or(Ident {
            name: String::new(),
            span: self
                .peek()
                .map(|t| self.span_of(t))
                .unwrap_or(Span::dummy()),
        });
        if self.at("::") {
            namespace.push(first.name.clone());
            self.eat("::");
            while let Some(part) = self.parse_name() {
                if self.eat("::") {
                    namespace.push(part.name);
                } else {
                    return (namespace, part);
                }
            }
        }
        (namespace, first)
    }

    fn parse_simple_type(&mut self) -> Option<TypeExpr> {
        if self.eat("(") {
            let ty = self.parse_type();
            self.expect(")");
            return ty;
        }
        if self.at("builtin") {
            let start = self.take().unwrap();
            self.expect("(");
            let params = if self.at(")") {
                Vec::new()
            } else {
                self.comma_separated(Self::parse_type)
            };
            self.expect(")");
            self.expect("=>");
            let result = self
                .parse_simple_type()
                .unwrap_or_else(|| self.void_type(self.span_of(start)));
            return Some(TypeExpr::Function {
                params,
                result: Box::new(result),
                span: self.span_of(start),
            });
        }
        let mutable = if self.at("const") {
            self.take();
            if self.eat("&") {
                let inner = self.parse_simple_type()?;
                return Some(TypeExpr::Reference {
                    mutable: false,
                    inner: Box::new(inner.clone()),
                    span: inner.span(),
                });
            }
            // `const` as a name fallback is not a type prefix here.
            None
        } else if self.eat("&") {
            let inner = self.parse_simple_type()?;
            return Some(TypeExpr::Reference {
                mutable: true,
                inner: Box::new(inner.clone()),
                span: inner.span(),
            });
        } else {
            Some(())
        };
        let _ = mutable;
        let is_constexpr = self.eat("constexpr");
        let (namespace, name) = self.parse_namespace_and_name();
        if name.name.is_empty() {
            return None;
        }
        let generic_args = self.parse_generic_args();
        Some(TypeExpr::Basic {
            is_constexpr,
            namespace,
            name,
            generic_args,
        })
    }

    fn void_type(&self, span: Span) -> TypeExpr {
        TypeExpr::Basic {
            is_constexpr: false,
            namespace: Vec::new(),
            name: Ident {
                name: "void".into(),
                span,
            },
            generic_args: Vec::new(),
        }
    }

    fn parse_type(&mut self) -> Option<TypeExpr> {
        let mut ty = self.parse_simple_type()?;
        while self.eat("|") {
            if let Some(right) = self.parse_simple_type() {
                ty = TypeExpr::Union(Box::new(ty), Box::new(right));
            } else {
                break;
            }
        }
        Some(ty)
    }

    fn parse_name_and_type(&mut self) -> Option<NameAndType> {
        let name = self.parse_name()?;
        self.expect(":");
        let ty = self.parse_type()?;
        Some(NameAndType { name, ty })
    }

    fn next_is_named_param(&mut self) -> bool {
        let saved = self.index;
        if matches!(
            self.peek().map(|t| t.text.as_str()),
            Some("implicit" | "js-implicit")
        ) {
            self.index = saved;
            return true;
        }
        let _ = self.parse_name();
        let named = self.at(":");
        self.index = saved;
        named
    }

    fn parse_param_list(&mut self) -> ParamList {
        let mut list = ParamList {
            implicit_kind: None,
            implicit: Vec::new(),
            named: Vec::new(),
            types_only: Vec::new(),
            rest: None,
        };
        if !self.at("(") {
            return list;
        }
        if matches!(
            self.peek().map(|t| t.text.as_str()),
            Some("implicit" | "js-implicit")
        ) || self.next_is_named_param() && self.looks_like_implicit_list()
        {
            // handled below
        }
        self.take(); // (
        if matches!(
            self.peek().map(|t| t.text.as_str()),
            Some("implicit" | "js-implicit")
        ) {
            list.implicit_kind = self.take().map(|t| t.text.clone());
            if !self.at(")") {
                list.implicit = self.comma_separated(Self::parse_name_and_type);
            }
            self.expect(")");
            if self.at("(") {
                let more = self.parse_param_list();
                list.named = more.named;
                list.types_only = more.types_only;
                list.rest = more.rest;
            }
            return list;
        }
        if self.at(")") {
            self.take();
            return list;
        }
        if self.next_is_named_param() {
            loop {
                if self.eat("...") {
                    list.rest = self.parse_name();
                    break;
                }
                if let Some(param) = self.parse_name_and_type() {
                    list.named.push(param);
                } else {
                    break;
                }
                if !self.eat(",") {
                    break;
                }
            }
        } else {
            loop {
                if self.eat("...") {
                    list.rest = self.parse_name();
                    break;
                }
                if let Some(ty) = self.parse_type() {
                    list.types_only.push(ty);
                } else {
                    break;
                }
                if !self.eat(",") {
                    break;
                }
                if self.eat("...") {
                    list.rest = self.parse_name();
                    break;
                }
            }
        }
        self.expect(")");
        list
    }

    fn looks_like_implicit_list(&mut self) -> bool {
        false
    }

    fn parse_label_list(&mut self) -> Vec<LabelParam> {
        if !self.eat("labels") {
            return Vec::new();
        }
        let mut labels = Vec::new();
        while let Some(name) = self.parse_name() {
            let types = if self.eat("(") {
                let types = if self.at(")") {
                    Vec::new()
                } else {
                    self.comma_separated(Self::parse_type)
                };
                self.expect(")");
                types
            } else {
                Vec::new()
            };
            labels.push(LabelParam { name, types });
            if !self.eat(",") {
                break;
            }
        }
        labels
    }

    fn parse_otherwise(&mut self) -> Vec<Stmt> {
        if !self.eat("otherwise") {
            return Vec::new();
        }
        self.comma_separated(Self::parse_atomar_statement)
    }

    fn parse_atomar_statement(&mut self) -> Option<Stmt> {
        if self.at("goto") {
            self.take();
            let label = self.parse_name()?;
            let args = if self.at("(") {
                self.parse_argument_list()
            } else {
                Vec::new()
            };
            return Some(Stmt::Goto { label, args });
        }
        if self.at("continue") {
            let start = self.take().unwrap();
            return Some(Stmt::Continue {
                span: self.span_of(start),
            });
        }
        if self.at("break") {
            let start = self.take().unwrap();
            return Some(Stmt::Break {
                span: self.span_of(start),
            });
        }
        if self.at("return") {
            let start = self.take().unwrap();
            let value = if self.at(",")
                || self.at(";")
                || self.at("}")
                || self.at(")")
                || self.peek().is_none()
            {
                None
            } else {
                self.parse_expression()
            };
            return Some(Stmt::Return {
                value,
                span: self.span_of(start),
            });
        }
        if matches!(
            self.peek().map(|t| t.text.as_str()),
            Some("debug" | "unreachable")
        ) {
            let token = self.take().unwrap();
            return Some(Stmt::Debug {
                kind: token.text.clone(),
                span: self.span_of(token),
            });
        }
        let expr = self.parse_expression()?;
        Some(Stmt::Expr(expr))
    }

    fn parse_argument_list(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        if !self.eat("(") {
            return args;
        }
        if !self.at(")") {
            args = self.comma_separated(Self::parse_expression);
        }
        self.expect(")");
        args
    }

    fn parse_ident_expr(&mut self) -> Option<Expr> {
        let saved = self.index;
        let mut namespace = Vec::new();
        if self.eat("::") {
            namespace.push(String::new());
        }
        let Some(first) = self.parse_name() else {
            self.index = saved;
            return None;
        };
        let mut name = first;
        while self.eat("::") {
            namespace.push(name.name.clone());
            if let Some(next) = self.parse_name() {
                name = next;
            } else {
                break;
            }
        }
        let generic_args = if self.at("<") && self.looks_like_generic_call() {
            self.parse_generic_args()
        } else {
            Vec::new()
        };
        Some(Expr::Ident {
            namespace,
            name,
            generic_args,
        })
    }

    fn looks_like_generic_call(&mut self) -> bool {
        let saved = self.index;
        let saved_extra = self.extra_gt;
        if !self.eat("<") {
            return false;
        }
        let mut depth = 1;
        let mut ok = false;
        while let Some(token) = self.take() {
            match token.text.as_str() {
                "<" => depth += 1,
                ">" => depth -= 1,
                ">>" => depth -= 2,
                ">>>" => depth -= 3,
                ";" | "{" | "}" => break,
                _ => {}
            }
            if depth <= 0 {
                ok = self.at("(")
                    || self.at("{")
                    || self.at(";")
                    || self.at(",")
                    || self.at(")")
                    || self.at("]");
                break;
            }
        }
        self.index = saved;
        self.extra_gt = saved_extra;
        ok
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        if self.eat("(") {
            let expr = self.parse_expression();
            self.expect(")");
            return expr;
        }
        if self.at("new") {
            let start = self.take().unwrap();
            while self.eat("(") {
                let _ = self.parse_name();
                self.expect(")");
            }
            let ty = self.parse_simple_type()?;
            let fields = self.parse_initializer_list();
            return Some(Expr::New {
                ty,
                fields,
                span: self.span_of(start),
            });
        }
        if self.at_kind(TokenKind::Intrinsic) {
            let name = self.parse_name()?;
            let generic_args = self.parse_generic_args();
            let args = self.parse_argument_list();
            return Some(Expr::IntrinsicCall {
                name,
                generic_args,
                args,
            });
        }
        if self.at_kind(TokenKind::Number) {
            let token = self.take().unwrap();
            if token.text.contains('.') {
                return Some(Expr::Float {
                    text: token.text.clone(),
                    span: self.span_of(token),
                });
            }
            return Some(Expr::Int {
                text: token.text.clone(),
                span: self.span_of(token),
            });
        }
        if self.at_kind(TokenKind::String) {
            let (value, span) = self.parse_string_value()?;
            return Some(Expr::String { value, span });
        }
        let ident = self.parse_ident_expr()?;
        Some(ident)
    }

    fn parse_initializer_list(&mut self) -> Vec<(Option<Ident>, Expr)> {
        let mut fields = Vec::new();
        if !self.eat("{") {
            return fields;
        }
        while !self.at("}") && self.peek().is_some() {
            let saved = self.index;
            if let Some(name) = self.parse_name()
                && self.eat(":")
            {
                if let Some(expr) = self.parse_expression() {
                    fields.push((Some(name), expr));
                }
                self.eat(",");
                continue;
            }
            self.index = saved;
            if let Some(expr) = self.parse_expression() {
                fields.push((None, expr));
            } else {
                break;
            }
            self.eat(",");
        }
        self.expect("}");
        fields
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.at("(") {
                let args = self.parse_argument_list();
                let otherwise = self.parse_otherwise();
                let span = expr.span();
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    otherwise,
                    span,
                };
                continue;
            }
            if self.eat(".") {
                let Some(field) = self.parse_name() else {
                    break;
                };
                if self.at("(") {
                    let args = self.parse_argument_list();
                    let otherwise = self.parse_otherwise();
                    let span = expr.span().merge(field.span);
                    expr = Expr::MethodCall {
                        target: Box::new(expr),
                        method: field,
                        args,
                        otherwise,
                        span,
                    };
                } else {
                    expr = Expr::Field {
                        object: Box::new(expr),
                        field,
                        via_ref: false,
                    };
                }
                continue;
            }
            if self.eat("->") {
                let Some(field) = self.parse_name() else {
                    break;
                };
                expr = Expr::Field {
                    object: Box::new(expr),
                    field,
                    via_ref: true,
                };
                continue;
            }
            if self.eat("[") {
                let index = self.parse_expression();
                self.expect("]");
                if let Some(index) = index {
                    let span = expr.span();
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                        span,
                    };
                }
                continue;
            }
            if self.at("++") || self.at("--") {
                let token = self.take().unwrap();
                let span = expr.span().merge(self.span_of(token));
                expr = Expr::IncDec {
                    pre: false,
                    inc: token.text == "++",
                    target: Box::new(expr),
                    span,
                };
                continue;
            }
            if self.at("{")
                && let Expr::Ident {
                    namespace,
                    name,
                    generic_args,
                } = &expr
            {
                let ty = TypeExpr::Basic {
                    is_constexpr: false,
                    namespace: namespace.clone(),
                    name: name.clone(),
                    generic_args: generic_args.clone(),
                };
                let span = expr.span();
                let fields = self.parse_initializer_list();
                expr = Expr::StructLit { ty, fields, span };
                continue;
            }
            break;
        }
        Some(expr)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if matches!(
            self.peek().map(|t| t.text.as_str()),
            Some("+" | "-" | "!" | "~" | "&")
        ) {
            let op = self.take().unwrap();
            let inner = self.parse_unary()?;
            let span = self.span_of(op).merge(inner.span());
            return Some(Expr::Call {
                callee: Box::new(Expr::Ident {
                    namespace: Vec::new(),
                    name: self.ident_from(op),
                    generic_args: Vec::new(),
                }),
                args: vec![inner],
                otherwise: Vec::new(),
                span,
            });
        }
        if self.eat("*") {
            let inner = self.parse_unary()?;
            let span = inner.span();
            return Some(Expr::Deref {
                inner: Box::new(inner),
                span,
            });
        }
        if self.eat("...") {
            let inner = self.parse_unary()?;
            let span = inner.span();
            return Some(Expr::Spread {
                inner: Box::new(inner),
                span,
            });
        }
        if self.at("++") || self.at("--") {
            let token = self.take().unwrap();
            let inner = self.parse_unary()?;
            let span = self.span_of(token).merge(inner.span());
            return Some(Expr::IncDec {
                pre: true,
                inc: token.text == "++",
                target: Box::new(inner),
                span,
            });
        }
        self.parse_postfix()
    }

    fn parse_binary(&mut self, min_prec: u8) -> Option<Expr> {
        let mut left = self.parse_unary()?;
        while let Some((op, prec, logical)) = self.peek().and_then(|t| binary_prec(&t.text)) {
            if prec < min_prec {
                break;
            }
            let op_token = self.take().unwrap();
            if op == "?" {
                let then_e = self.parse_expression()?;
                self.expect(":");
                let else_e = self.parse_binary(prec)?;
                let span = left.span().merge(else_e.span());
                left = Expr::Conditional {
                    cond: Box::new(left),
                    then_e: Box::new(then_e),
                    else_e: Box::new(else_e),
                    span,
                };
                continue;
            }
            let right = self.parse_binary(prec + 1)?;
            let span = left.span().merge(right.span());
            if logical {
                left = Expr::Logical {
                    op: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                    span,
                };
            } else {
                left = Expr::Call {
                    callee: Box::new(Expr::Ident {
                        namespace: Vec::new(),
                        name: Ident {
                            name: op.to_string(),
                            span: self.span_of(op_token),
                        },
                        generic_args: Vec::new(),
                    }),
                    args: vec![left, right],
                    otherwise: Vec::new(),
                    span,
                };
            }
        }
        Some(left)
    }

    fn parse_expression(&mut self) -> Option<Expr> {
        let left = self.parse_binary(1)?;
        if let Some(op) = self.peek().map(|t| t.text.clone())
            && is_assign_op(&op)
        {
            self.take();
            let value = self.parse_expression()?;
            let span = left.span().merge(value.span());
            return Some(Expr::Assign {
                target: Box::new(left),
                op: if op == "=" { None } else { Some(op) },
                value: Box::new(value),
                span,
            });
        }
        Some(left)
    }

    fn parse_block(&mut self) -> Option<Stmt> {
        let deferred = self.eat("deferred");
        let open = self.expect("{")?;
        let mut statements = Vec::new();
        while !self.at("}") && self.peek().is_some() {
            if let Some(stmt) = self.parse_statement() {
                statements.push(stmt);
            } else if !self.at("}") {
                self.take();
            }
        }
        let close = self.expect("}");
        let end = close.map(|t| t.end).unwrap_or(open.end);
        Some(Stmt::Block {
            deferred,
            statements,
            span: Span::new(self.file, open.start, end),
        })
    }

    fn parse_var_statement(&mut self) -> Option<Stmt> {
        let is_const = self.at("const");
        if !self.at("let") && !self.at("const") {
            return None;
        }
        let start = self.take().unwrap();
        let Some(name) = self.parse_name() else {
            self.error_here(format!("Expected identifier after '{}'", start.text));
            return None;
        };
        let ty = if self.eat(":") {
            self.parse_type()
        } else {
            None
        };
        let init = if self.eat("=") {
            self.parse_expression()
        } else {
            None
        };
        self.expect(";");
        Some(Stmt::Var {
            is_const,
            name,
            ty,
            init,
            span: self.span_of(start),
        })
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        if self.at_kind(TokenKind::Annotation) {
            self.parse_annotations();
            return self.parse_statement();
        }
        if self.at("deferred") || self.at("{") {
            return self.parse_block();
        }
        if self.at("let") || (self.at("const") && self.looks_like_var()) {
            return self.parse_var_statement();
        }
        if self.at("if") {
            let start = self.take().unwrap();
            let is_constexpr = self.eat("constexpr");
            self.expect("(");
            let cond = self.parse_expression()?;
            self.expect(")");
            let then_s = self.parse_statement()?;
            let else_s = if self.eat("else") {
                self.parse_statement().map(Box::new)
            } else {
                None
            };
            return Some(Stmt::If {
                is_constexpr,
                cond,
                then_s: Box::new(then_s),
                else_s,
                span: self.span_of(start),
            });
        }
        if self.at("while") {
            let start = self.take().unwrap();
            self.expect("(");
            let cond = self.parse_expression()?;
            self.expect(")");
            let body = self.parse_statement()?;
            return Some(Stmt::While {
                cond,
                body: Box::new(body),
                span: self.span_of(start),
            });
        }
        if self.at("for") {
            let start = self.take().unwrap();
            self.expect("(");
            let init = if !self.at(";") {
                self.parse_var_statement().map(Box::new)
            } else {
                None
            };
            if init.is_none() {
                self.eat(";");
            }
            let cond = if !self.at(";") {
                self.parse_expression()
            } else {
                None
            };
            self.expect(";");
            let step = if !self.at(")") {
                self.parse_expression()
            } else {
                None
            };
            self.expect(")");
            let body = self.parse_statement()?;
            return Some(Stmt::For {
                init,
                cond,
                step,
                body: Box::new(body),
                span: self.span_of(start),
            });
        }
        if self.at("typeswitch") {
            return self.parse_typeswitch();
        }
        if self.at("try") {
            let start = self.take().unwrap();
            let body = self.parse_block()?;
            let mut handlers = Vec::new();
            while self.at("label") || self.at("catch") {
                if self.at("label") {
                    self.take();
                    let name = self.parse_name()?;
                    let params = if self.at("(") {
                        self.parse_param_list()
                    } else {
                        ParamList {
                            implicit_kind: None,
                            implicit: Vec::new(),
                            named: Vec::new(),
                            types_only: Vec::new(),
                            rest: None,
                        }
                    };
                    let handler_body = self.parse_block()?;
                    handlers.push(TryHandler::Label {
                        name,
                        params,
                        body: Box::new(handler_body),
                    });
                } else {
                    self.take();
                    self.expect("(");
                    let mut names = Vec::new();
                    while let Some(name) = self.parse_name() {
                        names.push(name);
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.expect(")");
                    let handler_body = self.parse_block()?;
                    handlers.push(TryHandler::Catch {
                        names,
                        body: Box::new(handler_body),
                    });
                }
            }
            return Some(Stmt::Try {
                body: Box::new(body),
                handlers,
                span: self.span_of(start),
            });
        }
        if matches!(
            self.peek().map(|t| t.text.as_str()),
            Some("dcheck" | "check" | "sbxcheck" | "static_assert")
        ) {
            let token = self.take().unwrap();
            self.expect("(");
            let expr = self.parse_expression()?;
            self.expect(")");
            self.expect(";");
            return Some(Stmt::Assert {
                kind: token.text.clone(),
                expr,
                span: self.span_of(token),
            });
        }
        if self.at("return") {
            let start = self.take().unwrap();
            let value = if self.at(";") {
                None
            } else {
                self.parse_expression()
            };
            self.expect(";");
            return Some(Stmt::Return {
                value,
                span: self.span_of(start),
            });
        }
        if self.at("tail") {
            self.take();
            let expr = self.parse_expression()?;
            self.expect(";");
            return Some(Stmt::TailCall(expr));
        }
        if self.at("break") {
            let start = self.take().unwrap();
            self.expect(";");
            return Some(Stmt::Break {
                span: self.span_of(start),
            });
        }
        if self.at("continue") {
            let start = self.take().unwrap();
            self.expect(";");
            return Some(Stmt::Continue {
                span: self.span_of(start),
            });
        }
        if self.at("goto") {
            self.take();
            let label = self.parse_name()?;
            let args = if self.at("(") {
                self.parse_argument_list()
            } else {
                Vec::new()
            };
            self.expect(";");
            return Some(Stmt::Goto { label, args });
        }
        if matches!(
            self.peek().map(|t| t.text.as_str()),
            Some("debug" | "unreachable")
        ) {
            let token = self.take().unwrap();
            self.eat(";");
            return Some(Stmt::Debug {
                kind: token.text.clone(),
                span: self.span_of(token),
            });
        }
        let expr = self.parse_expression()?;
        self.expect(";");
        Some(Stmt::Expr(expr))
    }

    fn looks_like_var(&mut self) -> bool {
        let saved = self.index;
        self.take();
        let ok = self.parse_name().is_some() && (self.at(":") || self.at("=") || self.at(";"));
        self.index = saved;
        ok
    }

    fn parse_typeswitch(&mut self) -> Option<Stmt> {
        let start = self.take().unwrap();
        self.expect("(");
        let expr = self.parse_expression()?;
        self.expect(")");
        self.expect("{");
        let mut cases = Vec::new();
        while self.at("case") || self.at_kind(TokenKind::Annotation) {
            self.parse_annotations();
            if !self.eat("case") {
                break;
            }
            self.expect("(");
            let saved = self.index;
            let maybe_name = self.parse_name();
            let name = if self.eat(":") {
                maybe_name
            } else {
                self.index = saved;
                None
            };
            let ty = self.parse_type()?;
            self.expect(")");
            self.expect(":");
            let body = self.parse_block()?;
            cases.push(TypeswitchCase {
                name,
                ty,
                body: Box::new(body),
            });
        }
        self.expect("}");
        Some(Stmt::Typeswitch {
            expr,
            cases,
            span: self.span_of(start),
        })
    }

    fn parse_operator_name(&mut self) -> Option<String> {
        if !self.eat("operator") {
            return None;
        }
        if let Some((value, _)) = self.parse_string_value() {
            return Some(value);
        }
        self.take().map(|t| t.text.clone())
    }

    fn parse_return_type(&mut self) -> Option<TypeExpr> {
        if self.eat(":") {
            self.parse_type()
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_callable_after_kind(
        &mut self,
        annotations: Vec<Annotation>,
        transitioning: bool,
        javascript: bool,
        is_extern: bool,
        operator_name: Option<String>,
        kind: CallableKind,
        start_span: Span,
    ) -> Option<Decl> {
        let (_namespace, name) = self.parse_namespace_and_name();
        if name.name.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                start_span,
                format!("Expected {} name", callable_kind_name(kind)),
            ));
            return None;
        }
        let generic_params = self.parse_generic_params();
        let params = self.parse_param_list();
        let return_type = self.parse_return_type();
        let labels = self.parse_label_list();
        let body = if self.at("{") || self.at("deferred") {
            self.parse_block().map(Box::new)
        } else {
            self.eat(";");
            None
        };
        Some(Decl::Callable(CallableDecl {
            annotations,
            transitioning,
            javascript,
            is_extern,
            operator_name,
            kind,
            name,
            generic_params,
            params,
            return_type,
            labels,
            body,
        }))
    }

    fn parse_field(&mut self, bitfield: bool) -> Option<FieldDecl> {
        let weak = self.eat("weak");
        let is_const = self.eat("const");
        let name = self.parse_name()?;
        let _optional = self.eat("?");
        let index = if self.eat("[") {
            let expr = self.parse_expression();
            self.expect("]");
            expr
        } else {
            None
        };
        self.expect(":");
        let ty = self.parse_type()?;
        let bits = if bitfield && self.eat(":") {
            let token = self.take();
            self.eat("bit");
            token.and_then(|t| t.text.parse().ok())
        } else {
            None
        };
        self.expect(";");
        Some(FieldDecl {
            name,
            ty,
            weak,
            is_const,
            index,
            bits,
        })
    }

    fn parse_declaration(&mut self) -> Option<Decl> {
        let annotations = self.parse_annotations();
        if self.at_kind(TokenKind::Include) {
            self.take();
            if self.eat("[") {
                let _ = self.parse_string_value();
                self.expect("]");
            }
            let Some((path, span)) = self.parse_string_value() else {
                self.error_here("Expected string path after #include");
                return None;
            };
            return Some(Decl::Include { path, span });
        }
        if self.at_kind(TokenKind::Import) {
            self.take();
            let (path, span) = self.parse_string_value()?;
            return Some(Decl::Include { path, span });
        }
        if self.at("namespace") {
            self.take();
            let Some(name) = self.parse_name() else {
                self.error_here("Expected namespace name");
                return None;
            };
            self.expect("{");
            let mut body = Vec::new();
            while !self.at("}") && self.peek().is_some() {
                if let Some(decl) = self.parse_declaration() {
                    body.push(decl);
                } else if !self.at("}") {
                    self.take();
                }
            }
            if !self.eat("}") {
                self.diagnostics.push(Diagnostic::error(
                    name.span,
                    "Expected '}' to close namespace",
                ));
            }
            return Some(Decl::Namespace { name, body });
        }
        if self.at("bitfield") {
            self.take();
            if !self.eat("struct") {
                self.error_here("Expected 'struct' after 'bitfield'");
                return None;
            }
            let Some(name) = self.parse_name() else {
                self.error_here("Expected bitfield struct name");
                return None;
            };
            self.expect("extends");
            let extends = self.parse_type()?;
            self.expect("{");
            let mut fields = Vec::new();
            while !self.at("}") && self.peek().is_some() {
                self.parse_annotations();
                if let Some(field) = self.parse_field(true) {
                    fields.push(field);
                } else if !self.at("}") {
                    self.take();
                }
            }
            self.expect("}");
            return Some(Decl::BitFieldStruct {
                name,
                extends,
                fields,
                annotations,
            });
        }
        let is_extern = self.eat("extern");
        let transient = self.eat("transient");
        let transitioning = self.eat("transitioning");
        let javascript = self.eat("javascript");
        if self.at("class") || self.at("shape") {
            let is_shape = self.at("shape");
            let start = self.take().unwrap();
            let Some(name) = self.parse_name() else {
                self.diagnostics.push(Diagnostic::error(
                    self.span_of(start),
                    format!("Expected {} name", start.text),
                ));
                return None;
            };
            let extends = if self.eat("extends") {
                self.parse_type()
            } else {
                None
            };
            let generates = if self.eat("generates") {
                self.parse_string_value().map(|(s, _)| s)
            } else {
                None
            };
            let mut fields = Vec::new();
            let mut methods = Vec::new();
            if self.eat("{") {
                while !self.at("}") && self.peek().is_some() {
                    let inner_ann = self.parse_annotations();
                    if self.at("transitioning") || self.at("macro") || self.at("operator") {
                        let t = self.eat("transitioning");
                        let op = self.parse_operator_name();
                        if self.eat("macro")
                            && let Some(Decl::Callable(method)) = self.parse_callable_after_kind(
                                inner_ann,
                                t,
                                false,
                                false,
                                op,
                                CallableKind::Macro,
                                name.span,
                            )
                        {
                            methods.push(method);
                            continue;
                        }
                    }
                    if let Some(field) = self.parse_field(false) {
                        fields.push(field);
                    } else if !self.at("}") {
                        self.take();
                    }
                }
                self.expect("}");
            } else {
                self.eat(";");
            }
            return Some(Decl::Class {
                name,
                is_extern,
                is_shape,
                transient,
                extends,
                generates,
                fields,
                methods,
                annotations,
                span: self.span_of(start),
            });
        }
        if self.at("struct") {
            self.take();
            let Some(name) = self.parse_name() else {
                self.error_here("Expected struct name");
                return None;
            };
            let generic_params = self.parse_generic_params();
            self.expect("{");
            let mut fields = Vec::new();
            let mut methods = Vec::new();
            while !self.at("}") && self.peek().is_some() {
                let inner_ann = self.parse_annotations();
                if self.at("macro") || self.at("transitioning") || self.at("operator") {
                    let t = self.eat("transitioning");
                    let op = self.parse_operator_name();
                    if self.eat("macro")
                        && let Some(Decl::Callable(method)) = self.parse_callable_after_kind(
                            inner_ann,
                            t,
                            false,
                            false,
                            op,
                            CallableKind::Macro,
                            name.span,
                        )
                    {
                        methods.push(method);
                        continue;
                    }
                }
                if let Some(field) = self.parse_field(false) {
                    fields.push(field);
                } else if !self.at("}") {
                    self.take();
                }
            }
            self.expect("}");
            return Some(Decl::Struct {
                name,
                generic_params,
                fields,
                methods,
                annotations,
            });
        }
        if self.at("enum") {
            self.take();
            let Some(name) = self.parse_name() else {
                self.error_here("Expected enum name");
                return None;
            };
            let extends = if self.eat("extends") {
                self.parse_type()
            } else {
                None
            };
            if self.eat("constexpr") {
                let _ = self.parse_string_value();
            }
            self.expect("{");
            let mut entries = Vec::new();
            let mut is_open = false;
            while !self.at("}") && self.peek().is_some() {
                self.parse_annotations();
                if self.eat("...") {
                    is_open = true;
                    break;
                }
                if let Some(entry) = self.parse_name() {
                    let ty = if self.eat(":") {
                        self.parse_type()
                    } else {
                        None
                    };
                    entries.push((entry, ty));
                } else {
                    break;
                }
                if !self.eat(",") {
                    break;
                }
                if self.eat("...") {
                    is_open = true;
                    break;
                }
            }
            self.expect("}");
            return Some(Decl::Enum {
                name,
                extends,
                entries,
                is_open,
                annotations,
            });
        }
        if self.at("type") {
            let start = self.take().unwrap();
            let Some(name) = self.parse_name() else {
                self.diagnostics
                    .push(Diagnostic::error(self.span_of(start), "Expected type name"));
                return None;
            };
            let generic_params = self.parse_generic_params();
            if self.eat("=") {
                let ty = self.parse_type()?;
                self.expect(";");
                return Some(Decl::TypeAlias {
                    name,
                    generic_params,
                    ty,
                    annotations,
                });
            }
            let extends = if self.eat("extends") {
                self.parse_type()
            } else {
                None
            };
            let generates = if self.eat("generates") {
                self.parse_string_value().map(|(s, _)| s)
            } else {
                None
            };
            let constexpr_generates = if self.eat("constexpr") {
                self.parse_string_value().map(|(s, _)| s)
            } else {
                None
            };
            self.expect(";");
            return Some(Decl::AbstractType {
                name,
                generic_params,
                extends,
                generates,
                constexpr_generates,
                transient,
                annotations,
            });
        }
        if self.at("const") {
            self.take();
            let Some(name) = self.parse_name() else {
                self.error_here("Expected identifier after 'const'");
                return None;
            };
            self.expect(":");
            let ty = self.parse_type()?;
            if self.eat("generates") {
                let generates = self.parse_string_value().map(|(s, _)| s);
                self.expect(";");
                return Some(Decl::Const {
                    name,
                    ty,
                    init: None,
                    generates,
                    annotations,
                });
            }
            self.expect("=");
            let init = self.parse_expression();
            self.expect(";");
            return Some(Decl::Const {
                name,
                ty,
                init,
                generates: None,
                annotations,
            });
        }
        if self.at("intrinsic") {
            self.take();
            let span = self
                .peek()
                .map(|t| self.span_of(t))
                .unwrap_or(Span::dummy());
            return self.parse_callable_after_kind(
                annotations,
                transitioning,
                false,
                is_extern,
                None,
                CallableKind::Intrinsic,
                span,
            );
        }
        let operator_name = self.parse_operator_name();
        if self.at("macro") || self.at("builtin") || self.at("runtime") {
            if self.looks_like_qualified_runtime() {
                return None;
            }
            let token = self.take().unwrap();
            let kind = match token.text.as_str() {
                "macro" => CallableKind::Macro,
                "builtin" => CallableKind::Builtin,
                "runtime" => CallableKind::Runtime,
                _ => CallableKind::Macro,
            };
            return self.parse_callable_after_kind(
                annotations,
                transitioning,
                javascript,
                is_extern,
                operator_name,
                kind,
                self.span_of(token),
            );
        }
        if self.looks_like_bare_callable() {
            let span = self
                .peek()
                .map(|t| self.span_of(t))
                .unwrap_or(Span::dummy());
            return self.parse_callable_after_kind(
                annotations,
                transitioning,
                javascript,
                is_extern,
                operator_name,
                CallableKind::Macro,
                span,
            );
        }
        if is_extern || transitioning || javascript || operator_name.is_some() {
            self.error_here("Expected callable or type declaration");
        }
        None
    }

    fn looks_like_bare_callable(&mut self) -> bool {
        if !self.peek().is_some_and(|t| t.kind == TokenKind::Identifier) {
            return false;
        }
        let saved = self.index;
        let saved_extra = self.extra_gt;
        if self.parse_name().is_none() {
            self.index = saved;
            self.extra_gt = saved_extra;
            return false;
        }
        if !self.at("<") {
            self.index = saved;
            self.extra_gt = saved_extra;
            return false;
        }
        let _ = self.parse_generic_args();
        let ok = self.at("(");
        self.index = saved;
        self.extra_gt = saved_extra;
        ok
    }

    fn looks_like_qualified_runtime(&mut self) -> bool {
        if !self.at("runtime") {
            return false;
        }
        let saved = self.index;
        self.take();
        let qualified = self.at("::");
        self.index = saved;
        qualified
    }

    fn parse_file(&mut self) -> Vec<Decl> {
        let mut decls = Vec::new();
        while self.peek().is_some() {
            if let Some(decl) = self.parse_declaration() {
                decls.push(decl);
            } else if self.peek().is_some()
                && let Some(token) = self.take()
                && token.kind == TokenKind::Error
            {
                self.diagnostics.push(Diagnostic::error(
                    self.span_of(token),
                    token
                        .message
                        .clone()
                        .unwrap_or_else(|| "Invalid syntax".into()),
                ));
            }
        }
        decls
    }
}

fn callable_kind_name(kind: CallableKind) -> &'static str {
    match kind {
        CallableKind::Macro => "macro",
        CallableKind::Builtin => "builtin",
        CallableKind::Runtime => "runtime",
        CallableKind::Intrinsic => "intrinsic",
    }
}

fn binary_prec(op: &str) -> Option<(&'static str, u8, bool)> {
    match op {
        "?" => Some(("?", 2, false)),
        "||" => Some(("||", 3, true)),
        "&&" => Some(("&&", 4, true)),
        "&" | "|" | "^" => Some((leak(op), 5, false)),
        "==" | "!=" => Some((leak(op), 6, false)),
        "<" | ">" | "<=" | ">=" => Some((leak(op), 7, false)),
        "<<" | ">>" | ">>>" => Some((leak(op), 8, false)),
        "+" | "-" => Some((leak(op), 9, false)),
        "*" | "/" | "%" => Some((leak(op), 10, false)),
        _ => None,
    }
}

fn type_expr_as_ident(ty: &TypeExpr) -> Ident {
    match ty {
        TypeExpr::Basic {
            is_constexpr,
            namespace,
            name,
            generic_args,
        } if generic_args.is_empty() && namespace.is_empty() => {
            if *is_constexpr {
                Ident {
                    name: format!("constexpr {}", name.name),
                    span: name.span,
                }
            } else {
                name.clone()
            }
        }
        _ => Ident {
            name: type_expr_name(ty),
            span: ty.span(),
        },
    }
}

fn type_expr_name(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Basic {
            is_constexpr,
            namespace,
            name,
            generic_args,
        } => {
            let mut text = String::new();
            if *is_constexpr {
                text.push_str("constexpr ");
            }
            if !namespace.is_empty() {
                text.push_str(&namespace.join("::"));
                text.push_str("::");
            }
            text.push_str(&name.name);
            if !generic_args.is_empty() {
                text.push('<');
                text.push_str(
                    &generic_args
                        .iter()
                        .map(type_expr_name)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                text.push('>');
            }
            text
        }
        TypeExpr::Union(left, right) => {
            format!("{}|{}", type_expr_name(left), type_expr_name(right))
        }
        TypeExpr::Reference { mutable, inner, .. } => {
            if *mutable {
                format!("&{}", type_expr_name(inner))
            } else {
                format!("const &{}", type_expr_name(inner))
            }
        }
        TypeExpr::Function { .. } => "builtin".into(),
    }
}

fn leak(op: &str) -> &'static str {
    match op {
        "&" => "&",
        "|" => "|",
        "^" => "^",
        "==" => "==",
        "!=" => "!=",
        "<" => "<",
        ">" => ">",
        "<=" => "<=",
        ">=" => ">=",
        "<<" => "<<",
        ">>" => ">>",
        ">>>" => ">>>",
        "+" => "+",
        "-" => "-",
        "*" => "*",
        "/" => "/",
        "%" => "%",
        _ => "",
    }
}

fn is_assign_op(op: &str) -> bool {
    matches!(
        op,
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | ">>>="
    )
}

pub fn parse_file(uri: String, text: String, file: u32) -> ParseOutput {
    let tokens = tokenize(&text);
    let mut diagnostics: Vec<Diagnostic> = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Error)
        .map(|t| {
            Diagnostic::error(
                Span::new(file, t.start, t.end),
                t.message.clone().unwrap_or_else(|| "Invalid syntax".into()),
            )
        })
        .collect();
    for (start, end, message) in delimiter_errors(&tokens) {
        diagnostics.push(Diagnostic::error(Span::new(file, start, end), message));
    }
    let mut parser = Parser {
        file,
        tokens: &tokens,
        index: 0,
        extra_gt: 0,
        diagnostics,
    };
    let decls = parser.parse_file();
    let mut diagnostics = parser.diagnostics;
    let mut unique = std::collections::BTreeMap::new();
    for diagnostic in diagnostics.drain(..) {
        unique.insert(
            (diagnostic.start, diagnostic.end, diagnostic.message.clone()),
            diagnostic,
        );
    }
    ParseOutput {
        file: ParsedFile {
            uri,
            text,
            file,
            decls,
        },
        diagnostics: unique.into_values().collect(),
    }
}

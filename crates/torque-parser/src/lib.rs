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

#![allow(dead_code)]

mod earley_parser;
mod recovery;
mod torque_grammar;

use torque_ast::*;
use torque_diagnostic::Diagnostic;
use torque_span::Span;

use crate::torque_grammar::{MessageKind, parse_torque};

#[derive(Clone, Debug)]
pub struct TorqueImport {
    pub path: String,
    pub span: Span,
    pub diagnostic_index: usize,
}

#[derive(Clone, Debug)]
pub struct ParseOutput {
    pub file: ParsedFile,
    pub diagnostics: Vec<Diagnostic>,
    pub imports: Vec<TorqueImport>,
}

impl ParseOutput {
    pub fn diagnostics_with_imports(
        &self,
        is_in_source_set: impl Fn(&str) -> bool,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = self.diagnostics.clone();
        if let Some(import) = self
            .imports
            .iter()
            .find(|import| !is_in_source_set(&import.path))
        {
            diagnostics.truncate(import.diagnostic_index);
            diagnostics.push(Diagnostic::error(
                import.span,
                format!("File '{}' not found.", import.path),
            ));
            diagnostics.push(Diagnostic::error(
                import.span,
                format!("File '{}'is not part of the source set.", import.path),
            ));
        }
        diagnostics
    }
}

pub fn parse_file(uri: String, text: String, file: u32) -> ParseOutput {
    let result = parse_torque(&text, file);
    let decls = result
        .declarations
        .unwrap_or_else(|| recovery::parse_declarations(&text, file));
    let diagnostics = result
        .messages
        .into_iter()
        .map(|message| match message.kind {
            MessageKind::Error => Diagnostic::error(message.span, message.message),
            MessageKind::Lint => Diagnostic::warning(message.span, message.message),
        })
        .collect();
    ParseOutput {
        file: ParsedFile {
            uri,
            text,
            file,
            decls,
        },
        diagnostics,
        imports: result
            .imports
            .into_iter()
            .map(|import| TorqueImport {
                path: import.path,
                span: import.span,
                diagnostic_index: import.message_index,
            })
            .collect(),
    }
}

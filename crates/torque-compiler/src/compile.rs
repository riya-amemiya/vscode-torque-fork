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

use serde::{Deserialize, Serialize};

use crate::check::{FileAnalysis, check_files};
use crate::parser::parse_file;

#[derive(Clone, Debug, Deserialize)]
pub struct SourceFileInput {
    pub uri: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompileResult {
    pub files: Vec<FileAnalysis>,
}

pub fn compile(files: &[SourceFileInput]) -> CompileResult {
    let mut parsed = Vec::new();
    let mut parse_diagnostics = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let output = parse_file(file.uri.clone(), file.text.clone(), index as u32);
        parse_diagnostics.extend(output.diagnostics);
        parsed.push(output.file);
    }
    CompileResult {
        files: check_files(&parsed, parse_diagnostics),
    }
}

pub fn compile_json(input: &str) -> String {
    #[derive(Deserialize)]
    struct Input {
        files: Vec<SourceFileInput>,
    }
    match serde_json::from_str::<Input>(input) {
        Ok(parsed) => serde_json::to_string(&compile(&parsed.files))
            .unwrap_or_else(|_| "{\"files\":[]}".into()),
        Err(error) => serde_json::json!({
            "files": [{
                "uri": "",
                "diagnostics": [{
                    "message": format!("Invalid compiler input: {error}"),
                    "start": 0,
                    "end": 0,
                    "severity": "error",
                    "file": 0
                }],
                "symbols": [],
                "includes": [],
                "definitions": []
            }]
        })
        .to_string(),
    }
}

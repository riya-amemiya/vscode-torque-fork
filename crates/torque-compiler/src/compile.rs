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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use torque_check::{FileAnalysis, check_files};
use torque_parser::{ParseOutput, parse_file};

#[derive(Clone, Debug, Deserialize)]
pub struct SourceFileInput {
    pub uri: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    pub files: Vec<FileAnalysis>,
    pub parse_count: u32,
}

struct CachedParse {
    text: String,
    file: u32,
    output: ParseOutput,
}

thread_local! {
    static PARSE_CACHE: RefCell<HashMap<String, CachedParse>> = RefCell::new(HashMap::new());
    static PARSE_COUNT: Cell<u32> = const { Cell::new(0) };
}

fn parse_cached(uri: String, text: String, file: u32) -> ParseOutput {
    PARSE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(hit) = cache.get(&uri)
            && hit.text == text
            && hit.file == file
        {
            return hit.output.clone();
        }
        PARSE_COUNT.with(|count| count.set(count.get() + 1));
        let output = parse_file(uri.clone(), text.clone(), file);
        cache.insert(
            uri,
            CachedParse {
                text,
                file,
                output: output.clone(),
            },
        );
        output
    })
}

pub fn compile(files: &[SourceFileInput]) -> CompileResult {
    PARSE_COUNT.with(|count| count.set(0));
    let mut parsed = Vec::new();
    let mut parse_diagnostics = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let output = parse_cached(file.uri.clone(), file.text.clone(), index as u32);
        parse_diagnostics.extend(output.diagnostics);
        parsed.push(output.file);
    }
    CompileResult {
        files: check_files(&parsed, parse_diagnostics),
        parse_count: PARSE_COUNT.with(|count| count.get()),
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
            }],
            "parseCount": 0
        })
        .to_string(),
    }
}

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

use torque_check::{FileAnalysis, check_files_select, check_incremental, env_uri_index};
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
    static SOURCE_SET: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn is_in_source_set(uris: &[String], path: &str) -> bool {
    let suffix = format!("/{path}");
    uris.iter().any(|uri| uri == path || uri.ends_with(&suffix))
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
    compile_with_check(files, None)
}

pub fn compile_with_check(
    files: &[SourceFileInput],
    check_uris: Option<&[String]>,
) -> CompileResult {
    PARSE_COUNT.with(|count| count.set(0));
    let uris: Vec<String> = files.iter().map(|file| file.uri.clone()).collect();
    let mut parsed = Vec::new();
    let mut parse_diagnostics = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let output = parse_cached(file.uri.clone(), file.text.clone(), index as u32);
        parse_diagnostics
            .extend(output.diagnostics_with_imports(|path| is_in_source_set(&uris, path)));
        parsed.push(output.file);
    }
    SOURCE_SET.with(|set| *set.borrow_mut() = uris);
    CompileResult {
        files: check_files_select(&parsed, parse_diagnostics, check_uris),
        parse_count: PARSE_COUNT.with(|count| count.get()),
    }
}

pub fn compile_incremental(uri: &str, text: &str) -> Option<CompileResult> {
    let index = env_uri_index(uri)?;
    PARSE_COUNT.with(|count| count.set(0));
    let output = parse_cached(uri.to_string(), text.to_string(), index);
    let diagnostics = SOURCE_SET
        .with(|set| output.diagnostics_with_imports(|path| is_in_source_set(&set.borrow(), path)));
    let analysis = check_incremental(output.file, diagnostics)?;
    Some(CompileResult {
        files: vec![analysis],
        parse_count: PARSE_COUNT.with(|count| count.get()),
    })
}

pub fn compile_json(input: &str) -> String {
    #[derive(Deserialize)]
    struct Input {
        files: Vec<SourceFileInput>,
        #[serde(default, rename = "checkUris")]
        check_uris: Option<Vec<String>>,
        #[serde(default)]
        incremental: bool,
    }
    match serde_json::from_str::<Input>(input) {
        Ok(parsed) if parsed.incremental && parsed.files.len() == 1 => {
            let file = &parsed.files[0];
            serde_json::to_string(&compile_incremental(&file.uri, &file.text).unwrap_or(
                CompileResult {
                    files: Vec::new(),
                    parse_count: 0,
                },
            ))
            .unwrap_or_else(|_| "{\"files\":[],\"parseCount\":0}".into())
        }
        Ok(parsed) => serde_json::to_string(&compile_with_check(
            &parsed.files,
            parsed.check_uris.as_deref(),
        ))
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

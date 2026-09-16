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

use torque_compiler::{compile, SourceFileInput};

const SAMPLE: &str = r#"
namespace math {
  type Number = Smi | HeapNumber;
  extern class JSProxy extends JSReceiver {
    target: JSReceiver|Null;
    handler: JSReceiver|Null;
  }
  builtin HeapNumberIs42(implicit context: Context)(heapNumber: HeapNumber): Boolean {
    return Convert<float64>(heapNumber) == 42 ? True : False;
  }
  javascript builtin MathIs42(js-implicit context: NativeContext, receiver: JSAny)(x: JSAny): Boolean {
    const number: Number = ToNumber_Inline(x);
    typeswitch (number) {
      case (smi: Smi): {
        return smi == 42 ? True : False;
      }
      case (heapNumber: HeapNumber): {
        return HeapNumberIs42(heapNumber);
      }
    }
  }
}
"#;

fn compile_one(uri: &str, text: &str) -> torque_compiler::FileAnalysis {
    let result = compile(&[SourceFileInput {
        uri: uri.to_string(),
        text: text.to_string(),
    }]);
    result.files.into_iter().next().unwrap()
}

fn names(file: &torque_compiler::FileAnalysis) -> Vec<String> {
    file.symbols
        .iter()
        .map(|symbol| format!("{}:{}", symbol.kind, symbol.name))
        .collect()
}

#[test]
fn extracts_namespaces_types_classes_fields_and_builtins() {
    let file = compile_one("memory://sample.tq", SAMPLE.trim());
    let names = names(&file);
    assert!(names.iter().any(|n| n == "namespace:math"), "{names:?}");
    assert!(names.iter().any(|n| n == "type:Number"), "{names:?}");
    assert!(names.iter().any(|n| n == "class:JSProxy"), "{names:?}");
    assert!(names.iter().any(|n| n == "field:target"), "{names:?}");
    assert!(
        names.iter().any(|n| n == "builtin:HeapNumberIs42"),
        "{names:?}"
    );
    assert!(names.iter().any(|n| n == "builtin:MathIs42"), "{names:?}");
    assert!(names.iter().any(|n| n == "const:number"), "{names:?}");
    assert!(!names.iter().any(|n| n == "field:True"), "{names:?}");
}

#[test]
fn detects_unmatched_braces() {
    let file = compile_one("memory://broken.tq", "macro Broken(): void { if (true) {");
    assert!(
        file.diagnostics
            .iter()
            .any(|item| item.message.contains("Unclosed")),
        "{:?}",
        file.diagnostics
    );
}

#[test]
fn detects_missing_macro_name() {
    let file = compile_one("memory://broken.tq", "macro (): void {}");
    assert!(
        file.diagnostics
            .iter()
            .any(|item| item.message == "Expected macro name"),
        "{:?}",
        file.diagnostics
    );
}

#[test]
fn does_not_treat_runtime_calls_as_declarations() {
    let source = r#"
transitioning macro ArrayIsArray_Inline(
    implicit context: Context)(element: JSAny): Boolean {
  return Cast<Boolean>(runtime::ArrayIsArray(element)) otherwise unreachable;
}
"#;
    let file = compile_one("memory://runtime.tq", source.trim());
    assert!(
        !file
            .diagnostics
            .iter()
            .any(|item| item.message == "Expected runtime name"),
        "{:?}",
        file.diagnostics
    );
    assert!(names(&file)
        .iter()
        .any(|n| n == "macro:ArrayIsArray_Inline"));
}

#[test]
fn records_include_paths() {
    let file = compile_one("memory://inc.tq", "#include \"src/objects/js-proxy.h\"\n");
    assert_eq!(file.includes.len(), 1);
    assert_eq!(file.includes[0].path, "src/objects/js-proxy.h");
}

#[test]
fn jumps_from_call_to_builtin_declaration() {
    let text = SAMPLE.trim();
    let file = compile_one("memory://sample.tq", text);
    let from = text.rfind("HeapNumberIs42").unwrap() as u32;
    let hit = file
        .definitions
        .iter()
        .find(|item| from >= item.from_start && from <= item.from_end)
        .expect("definition mapping for HeapNumberIs42 call");
    let decl = text.find("HeapNumberIs42").unwrap() as u32;
    assert_eq!(hit.to_start, decl);
}

#[test]
fn reports_type_mismatch_as_build_error() {
    let source = r#"
macro Wrong(x: Smi): String {
  return x;
}
"#;
    let file = compile_one("memory://mismatch.tq", source.trim());
    assert!(
        file.diagnostics
            .iter()
            .any(|item| item.message.contains("not assignable")),
        "{:?}",
        file.diagnostics
    );
}

#[test]
fn reports_failed_generic_inference() {
    let source = r#"
macro Pick<T: type>(x: T, y: T): T { return x; }
macro Main(a: Smi, b: String): Smi {
  return Pick(a, b);
}
"#;
    let file = compile_one("memory://infer.tq", source.trim());
    assert!(
        file.diagnostics
            .iter()
            .any(|item| item.message.contains("conflicting types")),
        "{:?}",
        file.diagnostics
    );
}

#[test]
fn reports_uninferable_generic_arguments() {
    let source = r#"
macro Identity<T: type>(): T;
macro Main(): Smi {
  return Identity();
}
"#;
    let file = compile_one("memory://uninfer.tq", source.trim());
    assert!(
        file.diagnostics.iter().any(|item| item
            .message
            .contains("failed to infer arguments for all type parameters")),
        "{:?}",
        file.diagnostics
    );
}

#[test]
fn infers_generic_from_compatible_arguments() {
    let source = r#"
macro Pick<T: type>(x: T, y: T): T { return x; }
macro Main(a: Smi): Smi {
  return Pick(1, a);
}
"#;
    let file = compile_one("memory://infer-ok.tq", source.trim());
    assert!(
        !file.diagnostics.iter().any(|item| {
            item.message.contains("conflicting types") || item.message.contains("failed to infer")
        }),
        "{:?}",
        file.diagnostics
    );
    let text = source.trim();
    let from = text.rfind("Pick").unwrap() as u32;
    let hit = file
        .definitions
        .iter()
        .find(|item| from >= item.from_start && from <= item.from_end)
        .expect("definition mapping for inferred Pick call");
    assert_eq!(hit.to_start, text.find("Pick").unwrap() as u32);
}

#[test]
fn reports_unannotated_uninitialized_let_as_inference_error() {
    let source = r#"
macro Main(): void {
  let mystery;
}
"#;
    let file = compile_one("memory://let.tq", source.trim());
    assert!(
        file.diagnostics
            .iter()
            .any(|item| item.message.contains("Cannot infer type of 'mystery'")),
        "{:?}",
        file.diagnostics
    );
}

#[test]
fn resolves_across_files() {
    let helper = r#"
macro Helper(x: Smi): Smi { return x; }
"#;
    let main = r#"
macro Main(x: Smi): Smi { return Helper(x); }
"#;
    let result = compile(&[
        SourceFileInput {
            uri: "memory://helper.tq".into(),
            text: helper.trim().into(),
        },
        SourceFileInput {
            uri: "memory://main.tq".into(),
            text: main.trim().into(),
        },
    ]);
    let main_file = result
        .files
        .iter()
        .find(|file| file.uri == "memory://main.tq")
        .unwrap();
    let offset = main.trim().find("Helper").unwrap() as u32;
    let hit = main_file
        .definitions
        .iter()
        .find(|item| offset >= item.from_start && offset <= item.from_end)
        .expect("cross-file definition");
    assert_eq!(hit.to_uri, "memory://helper.tq");
}

#[test]
fn jumps_from_operator_use_to_operator_macro() {
    let source = r#"
extern operator '==' macro SmiEqual(x: Smi, y: Smi): bool;
macro Main(a: Smi): bool {
  return a == 1;
}
"#;
    let file = compile_one("memory://op.tq", source.trim());
    let text = source.trim();
    let from = text.rfind("==").unwrap() as u32;
    let hit = file
        .definitions
        .iter()
        .find(|item| from >= item.from_start && from <= item.from_end)
        .expect("definition mapping for ==");
    assert_eq!(hit.to_start, text.find("SmiEqual").unwrap() as u32);
}

#[test]
fn jumps_from_type_use_to_type_declaration() {
    let source = r#"
type Number = Smi | HeapNumber;
macro Main(x: Number): Number { return x; }
"#;
    let file = compile_one("memory://type.tq", source.trim());
    let text = source.trim();
    let from = text.rfind("Number").unwrap() as u32;
    let hit = file
        .definitions
        .iter()
        .find(|item| from >= item.from_start && from <= item.from_end)
        .expect("definition mapping for Number");
    assert_eq!(hit.to_start, text.find("Number").unwrap() as u32);
}

#[test]
fn compile_json_returns_symbols_and_errors() {
    let json = torque_compiler::compile_json(
        r#"{"files":[{"uri":"memory://a.tq","text":"macro Wrong(x: Smi): String { return x; }"}]}"#,
    );
    assert!(json.contains("Wrong"), "{json}");
    assert!(json.contains("not assignable"), "{json}");
    assert!(json.contains("fromStart") || json.contains("from_start") || json.contains("symbols"));
}

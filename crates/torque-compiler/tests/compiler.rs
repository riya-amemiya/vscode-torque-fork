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

use torque_compiler::{SourceFileInput, compile};

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
    assert!(
        names(&file)
            .iter()
            .any(|n| n == "macro:ArrayIsArray_Inline")
    );
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

#[test]
fn object_alias_to_heapobject_union_does_not_overflow() {
    let source = r#"
type Object = Smi | HeapObject;
extern class HeapObject extends Object {
  map: Map;
}
extern class Map extends HeapObject {}
macro Main(x: HeapObject): Map {
  return x.map;
}
"#;
    let file = compile_one("memory://cycle.tq", source.trim());
    assert!(
        names(&file).iter().any(|name| name == "class:HeapObject"),
        "{:?}",
        names(&file)
    );
    let from = source.trim().rfind(".map").unwrap() as u32 + 1;
    let hit = file
        .definitions
        .iter()
        .find(|item| from >= item.from_start && from <= item.from_end);
    assert!(
        hit.is_some(),
        "expected field jump, diags={:?}",
        file.diagnostics
    );
}

fn messages(file: &torque_compiler::FileAnalysis) -> Vec<String> {
    file.diagnostics
        .iter()
        .map(|item| item.message.clone())
        .collect()
}

fn is_parser_garbage(message: &str) -> bool {
    message == "Expected ';'"
        || message == "Expected ')'"
        || message == "Cannot resolve 'this'"
        || message == "Cannot resolve 'goto'"
        || message == "Cannot resolve 'continue'"
        || message == "Cannot resolve 'arguments'"
        || message == "Cannot resolve 'context'"
        || message.contains("Cannot compare 'Smi' with 'IntegerLiteral'")
        || message.contains("Cannot compare 'intptr' with 'IntegerLiteral'")
        || message.contains("Cannot compare 'Smi' with 'constexpr IntegerLiteral'")
        || message.contains("Cannot compare 'intptr' with 'constexpr IntegerLiteral'")
        || message.contains("Type 'IntegerLiteral' is not assignable to 'Smi'")
        || message.contains("Type 'IntegerLiteral' is not assignable to 'intptr'")
        || message.contains("Type 'constexpr IntegerLiteral' is not assignable to 'Smi'")
        || message.contains("Type 'constexpr IntegerLiteral' is not assignable to 'intptr'")
}

#[test]
fn struct_methods_bind_this_and_struct_literals() {
    let source = r#"
struct FlatVector {
  macro CreateJSArray(implicit context: Context)(targetKind: ElementsKind): JSAny {
    const a: JSAny = this.fixedArray;
    this.fixedArray = a;
    return a;
  }
  fixedArray: JSAny;
}
macro NewFlatVector(implicit context: Context)(length: Smi): FlatVector {
  const empty: JSAny = length > 0 ? length : 0;
  return FlatVector{fixedArray: empty};
}
"#;
    let file = compile_one("memory://flat-vector.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| is_parser_garbage(item)),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("Cannot resolve 'this'")),
        "{messages:?}"
    );
}

#[test]
fn otherwise_goto_continue_and_try_labels_are_statements() {
    let source = r#"
struct FastJSArrayWitness {
  macro Recheck(): void labels CastError {}
  macro Get(): FastJSArrayWitness { return this; }
  macro LoadElementNoHole(index: Smi): JSAny labels FoundHole { return index; }
  length: Smi;
}
macro Flatten(implicit context: Context)(source: FastJSArrayWitness, length: Smi):
    JSAny labels Bailout {
  let index: Smi = 0;
  source.Recheck() otherwise goto Bailout;
  try {
    const element: JSAny = source.LoadElementNoHole(index) otherwise FoundHole;
  } label FoundHole {
    index = index;
  }
  if (index >= source.Get().length) goto Bailout;
  const skipped: JSAny = source.LoadElementNoHole(index) otherwise continue;
  return skipped;
}
"#;
    let file = compile_one("memory://otherwise.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| is_parser_garbage(item)
            || item.contains("Cannot resolve 'FoundHole'")
            || item.contains("Cannot resolve label")),
        "{messages:?}"
    );
}

#[test]
fn javascript_rest_arguments_and_js_implicit_context_are_bound() {
    let source = r#"
extern macro ArraySpeciesCreate(context: NativeContext, o: JSReceiver, length: Number):
    JSReceiver;
transitioning javascript builtin ArrayPrototypeFlat(
    js-implicit context: NativeContext, receiver: JSAny)(...arguments): JSAny {
  const o: JSReceiver = receiver;
  if (arguments[0] != Undefined) {
    return arguments[0];
  }
  const a: JSReceiver = ArraySpeciesCreate(context, o, 0);
  return a;
}
"#;
    let file = compile_one("memory://js-builtin.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| is_parser_garbage(item)),
        "{messages:?}"
    );
}

#[test]
fn js_receiver_is_assignable_to_jsany_and_compares_with_undefined() {
    let source = r#"
macro Main(o: JSReceiver, x: JSAny): JSAny {
  if (x != Undefined) {
    return o;
  }
  return x;
}
"#;
    let file = compile_one("memory://jsany.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("not assignable") || item.contains("Cannot compare")),
        "{messages:?}"
    );
}

#[test]
fn integer_literals_compare_and_assign_to_numeric_types() {
    let source = r#"
const kMaxFlatFastStackEntries: intptr = 3072;
macro Main(length: Smi, stackLength: intptr, n: Number): bool {
  let target: Smi = 0;
  return length > 0 && stackLength == 0 && n >= 9007199254740991.0;
}
"#;
    let file = compile_one("memory://lits.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("not assignable") || item.contains("Cannot compare")),
        "{messages:?}"
    );
}

#[test]
fn qualified_enum_entries_and_dot_operators_resolve() {
    let source = r#"
extern enum ElementsKind {
  PACKED_SMI_ELEMENTS,
  PACKED_DOUBLE_ELEMENTS,
  PACKED_ELEMENTS
}
extern class FixedArrayBase extends HeapObject {
  length: Smi;
}
extern class FixedArray extends FixedArrayBase {}
extern operator '.length_intptr' macro LoadAndUntagFixedArrayBaseLength(FixedArrayBase): intptr;
extern operator '.objects[]' macro LoadFixedArrayElement(FixedArray, Smi): Object;
extern operator '.objects[]=' macro StoreFixedArrayElement(FixedArray, Smi, JSAny): void;
extern operator '.elements_kind' macro LoadMapElementsKind(Map): ElementsKind;
macro ReadKind(map: Map, array: FixedArray, index: Smi, value: JSAny): ElementsKind {
  array.objects[index] = value;
  const loaded: Object = array.objects[index];
  const len: intptr = array.length_intptr;
  return map.elements_kind == ElementsKind::PACKED_SMI_ELEMENTS ?
      ElementsKind::PACKED_ELEMENTS : ElementsKind::PACKED_DOUBLE_ELEMENTS;
}
"#;
    let file = compile_one("memory://ops.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("has no field")
            || item.contains("Cannot resolve 'PACKED")
            || item.contains("Cannot resolve 'k")),
        "{messages:?}"
    );
}

#[test]
fn does_not_cascade_field_errors_on_unresolved_receivers() {
    let source = r#"
macro Main(x: MissingType): Smi {
  return x.fixedArray;
}
"#;
    let file = compile_one("memory://cascade.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("has no field 'fixedArray'")),
        "{messages:?}"
    );
}

#[test]
fn array_flat_shaped_v8_builtins_do_not_emit_parser_garbage() {
    let source = r#"
extern enum ElementsKind { PACKED_SMI_ELEMENTS, PACKED_DOUBLE_ELEMENTS, PACKED_ELEMENTS }
extern enum MessageTemplate { kFlattenPastSafeLength }
extern class FastJSArray extends JSObject { length: Number; map: Map; }
extern class FastJSArrayForRead extends FastJSArray {}
extern class FastJSArrayWitness {
  macro Recheck(): void labels CastError {}
  macro Get(): FastJSArray { return this.array; }
  macro LoadElementNoHole(index: Smi): JSAny labels FoundHole { return index; }
  array: FastJSArray;
}
extern class GrowableFixedArray {
  macro Push(v: JSAny): void {}
  length: intptr;
  array: FixedArray;
}
extern class FixedArray extends HeapObject {}
extern operator '.length_intptr' macro LoadLen(FixedArray): intptr;
extern operator '.objects[]' macro LoadObj(FixedArray, Smi): Object;
extern operator '.elements_kind' macro LoadKind(Map): ElementsKind;
extern macro TrySmiAdd(x: Smi, y: Smi): Smi labels Overflow;
extern macro TrySmiSub(x: Smi, y: Smi): Smi labels Overflow;
extern macro ArraySpeciesCreate(context: NativeContext, o: JSReceiver, length: Number): JSReceiver;
extern macro NewGrowableFixedArray(): GrowableFixedArray;
const kMaxFlatFastStackEntries: intptr = 3072;
struct FlatVector {
  macro CreateJSArray(implicit context: Context)(targetKind: ElementsKind): JSAny {
    return this.fixedArray;
  }
  macro StoreResult(implicit context: Context)(index: Smi, result: JSAny): void {
    this.fixedArray.objects[index] = result;
  }
  fixedArray: FixedArray;
}
macro NewFlatVector(implicit context: Context)(length: Smi): FlatVector {
  return FlatVector{fixedArray: kEmptyFixedArray};
}
transitioning macro FlattenIntoArrayFast(
    implicit context: Context)(source: FastJSArray, sourceLength: Number,
    depth: Smi): Number labels Bailout(Number, Number) {
  const fastLength: Smi = Cast<Smi>(sourceLength) otherwise goto Bailout;
  let stack = NewGrowableFixedArray();
  let fastOW = source;
  let index: Smi = 0;
  fastOW.Recheck() otherwise goto Bailout(0, 0);
  try {
    const element: JSAny = fastOW.LoadElementNoHole(index) otherwise FoundHole;
  } label FoundHole {
    index++;
  }
  if (stack.length >= kMaxFlatFastStackEntries) goto Bailout(0, 0);
  stack.Push(source);
  const next: Smi = TrySmiAdd(index, 1) otherwise goto Bailout(0, 0);
  const kind: ElementsKind = source.map.elements_kind;
  if (kind == ElementsKind::PACKED_SMI_ELEMENTS) {
    return 0;
  }
  return next;
}
transitioning javascript builtin ArrayPrototypeFlat(
    js-implicit context: NativeContext, receiver: JSAny)(...arguments): JSAny {
  const o: JSReceiver = receiver;
  if (arguments[0] != Undefined) {
    return arguments[0];
  }
  const a: JSReceiver = ArraySpeciesCreate(context, o, 0);
  return a;
}
"#;
    let file = compile_one("memory://array-flat-shaped.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| is_parser_garbage(item)),
        "{messages:?}"
    );
}

#[test]
fn dump_real_iterator() {
    let path = "/tmp/v8-full-tq/src/builtins/iterator.tq";
    if std::fs::read_to_string(path).is_err() {
        return;
    }
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, files: &mut Vec<SourceFileInput>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("tq") {
                files.push(SourceFileInput {
                    uri: path.to_string_lossy().into_owned(),
                    text: std::fs::read_to_string(&path).unwrap_or_default(),
                });
            }
        }
    }
    walk(std::path::Path::new("/tmp/v8-full-tq"), &mut files);
    let result = compile(&files);
    let Some(file) = result
        .files
        .iter()
        .find(|item| item.uri.ends_with("src/builtins/iterator.tq"))
    else {
        return;
    };
    assert_no_false_positives(file);
    assert!(file.diagnostics.is_empty(), "{:?}", messages(file));
}

#[test]
fn dump_real_base_has_no_user_reported_garbage() {
    let path = "/tmp/v8-full-tq/src/builtins/base.tq";
    if std::fs::read_to_string(path).is_err() {
        return;
    }
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, files: &mut Vec<SourceFileInput>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("tq") {
                files.push(SourceFileInput {
                    uri: path.to_string_lossy().into_owned(),
                    text: std::fs::read_to_string(&path).unwrap_or_default(),
                });
            }
        }
    }
    walk(std::path::Path::new("/tmp/v8-full-tq"), &mut files);
    let result = compile(&files);
    let Some(file) = result
        .files
        .iter()
        .find(|item| item.uri.ends_with("src/builtins/base.tq"))
    else {
        return;
    };
    assert_no_false_positives(file);
    let messages = messages(file);
    let garbage: Vec<_> = messages
        .iter()
        .filter(|item| {
            item.contains("Expected '>'")
                || item.contains("Cannot resolve 'return'")
                || item.contains("Cannot resolve type 'V8_ENABLE")
                || item.contains("Cannot resolve 'dcheck'")
                || item.contains("Cannot resolve 'Slow'")
                || item.contains("Cannot resolve label")
                || item.contains("has no field")
                || item.contains("MakeWeak")
                || item.contains("GetHeapObjectAssumeWeak")
                || item.contains("SmiFromUint32")
                || item.contains("not assignable")
        })
        .cloned()
        .collect();
    assert!(garbage.is_empty(), "{messages:?}");
}

#[test]
fn dump_real_array_flat() {
    let path = "/tmp/v8-tq/array-flat.tq";
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let file = compile_one("memory://array-flat.tq", &text);
    let garbage: Vec<_> = messages(&file)
        .into_iter()
        .filter(|item| is_parser_garbage(item))
        .collect();
    assert!(garbage.is_empty(), "{garbage:?}");
}

fn assert_no_false_positives(file: &torque_compiler::FileAnalysis) {
    let messages = messages(file);
    let garbage: Vec<_> = messages
        .iter()
        .filter(|item| {
            is_parser_garbage(item)
                || item.contains("IntegerLiteral' is not assignable")
                || item.contains("constexpr string' is not assignable")
                || item.contains("Missing return value")
                || item.contains("Cannot resolve 'IteratorStep'")
                || item.contains("Cannot resolve 'IteratorValue'")
                || item.contains("Cannot resolve 'GetIterator'")
                || item.contains("Cannot find matching callable 'CollectCallFeedback'")
                || item.contains("Cannot find matching callable 'ThrowIfNotJSReceiver'")
                || item.contains("Cannot find matching callable 'ThrowTypeError'")
                || item.contains("Cannot find matching callable 'NativeContextSlot'")
                || item.contains("Cannot compare 'int31' with 'IteratorRecord'")
        })
        .cloned()
        .collect();
    assert!(garbage.is_empty(), "{messages:?}");
}

#[test]
fn v8_void_and_integer_literal_types_keep_literals_and_bare_returns() {
    let source = r#"
type IntegerLiteral constexpr 'IntegerLiteral';
type void;
type never;
type int31 extends int32;
type intptr generates 'IntPtrT' constexpr 'intptr_t';
const kCount: constexpr int31 = 2;
macro Early(x: JSAny): void {
  if (x == Undefined) return;
}
macro Main(): intptr {
  let i: intptr = 0;
  Early(Undefined);
  return i;
}
"#;
    let file = compile_one("memory://lits-v8.tq", source.trim());
    assert_no_false_positives(&file);
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("not assignable") || item.contains("Missing return")),
        "{messages:?}"
    );
}

#[test]
fn cpp_assembler_macros_are_called_by_their_method_name() {
    let source = r#"
struct IteratorRecord {
  object: JSReceiver;
  next: JSAny;
}
extern transitioning macro IteratorBuiltinsAssembler::GetIterator(
    implicit context: Context)(JSAny): IteratorRecord;
extern transitioning macro IteratorBuiltinsAssembler::IteratorStep(
    implicit context: Context)(IteratorRecord): JSReceiver
    labels Done;
extern transitioning macro IteratorBuiltinsAssembler::IteratorValue(
    implicit context: Context)(JSReceiver): JSAny;
macro Walk(implicit context: Context)(value: JSAny): JSAny labels Done {
  const iterated = GetIterator(value);
  const result = IteratorStep(iterated) otherwise Done;
  return IteratorValue(result);
}
"#;
    let file = compile_one("memory://assembler.tq", source.trim());
    assert_no_false_positives(&file);
    assert!(
        names(&file).iter().any(|item| item == "macro:IteratorStep"),
        "{:?}",
        names(&file)
    );
}

#[test]
fn size_of_generic_returns_int31_not_the_type_argument() {
    let source = r#"
struct IteratorRecord {
  object: JSReceiver;
}
const kCount: constexpr int31 = 2;
const kTaggedSize: constexpr int31 = 8;
macro SizeOf<T: type>(): constexpr int31 {
  return 16;
}
macro Main(): bool {
  return kCount * kTaggedSize == SizeOf<IteratorRecord>();
}
"#;
    let file = compile_one("memory://sizeof.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("Cannot compare") || item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn string_literals_assign_to_constexpr_string_and_match_overloads() {
    let source = r#"
type string constexpr 'const char*';
extern enum MessageTemplate {
  kCalledOnNonObject,
  kSymbolIteratorInvalid
}
extern macro ThrowTypeError(
    implicit context: Context)(constexpr MessageTemplate,
    constexpr string): never;
extern transitioning macro ThrowIfNotJSReceiver(
    implicit context: Context)(JSAny, constexpr MessageTemplate,
    constexpr string): void;
macro Main(implicit context: Context)(value: JSAny): void {
  const methodName: constexpr string = 'Iterator';
  ThrowIfNotJSReceiver(value, MessageTemplate::kSymbolIteratorInvalid, '');
  ThrowTypeError(MessageTemplate::kCalledOnNonObject, methodName);
}
"#;
    let file = compile_one("memory://strings.tq", source.trim());
    assert_no_false_positives(&file);
}

#[test]
fn make_lazy_produces_lazy_and_matches_collect_call_feedback() {
    let source = r#"
type string constexpr 'const char*';
type Lazy<T: type>;
intrinsic %MakeLazy<T: type, A1: type>(
    getter: constexpr string, arg1: A1): Lazy<T>;
macro CollectCallFeedback(
    maybeTarget: JSAny, maybeReceiver: Lazy<JSAny>, context: Context,
    slotId: uintptr): void {}
macro GetLazyReceiver(receiver: JSAny): JSAny {
  return receiver;
}
macro Main(iteratorMethod: JSAny, receiver: JSAny, context: Context,
    slotId: uintptr): void {
  CollectCallFeedback(
      iteratorMethod, %MakeLazy<JSAny, JSAny>('GetLazyReceiver', receiver),
      context, slotId);
}
"#;
    let file = compile_one("memory://lazy.tq", source.trim());
    assert_no_false_positives(&file);
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("CollectCallFeedback") || item.contains("MakeLazy")),
        "{messages:?}"
    );
}

#[test]
fn native_context_slot_infers_t_from_enum_entry_slot_type() {
    let source = r#"
type Slot<Container: type, T: type> extends intptr;
extern class JSFunction extends JSReceiver {}
extern enum ContextSlot extends intptr constexpr 'Context::Field' {
  PROMISE_FUNCTION_INDEX: Slot<NativeContext, JSFunction>,
}
macro NativeContextSlot<C: type, T: type>(
    implicit context: C)(index: Slot<NativeContext, T>): T {
  return %RawDownCast<T>(index);
}
macro Main(implicit context: Context)(): JSFunction {
  return *NativeContextSlot(ContextSlot::PROMISE_FUNCTION_INDEX);
}
"#;
    let file = compile_one("memory://slot.tq", source.trim());
    assert_no_false_positives(&file);
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("NativeContextSlot") || item.contains("PROMISE_FUNCTION")),
        "{messages:?}"
    );
}

fn assert_clean(file: &torque_compiler::FileAnalysis) {
    let messages = messages(file);
    assert!(
        !messages.iter().any(|item| item.contains("Expected '>'")
            || item.contains("Expected '{'")
            || item.contains("Cannot resolve 'return'")
            || item.contains("Cannot resolve type 'V8_ENABLE")
            || item.contains("Cannot resolve 'dcheck'")
            || item.contains("Cannot resolve 'Slow'")
            || item.contains("Cannot resolve label")
            || item.contains("has no field")
            || item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn nested_generic_closing_angles_are_not_shift_tokens() {
    let source = r#"
type WeakHeapObject;
type Weak<T: type> extends WeakHeapObject;
type MaybeObject = Smi|HeapObject|WeakHeapObject;
type RawPtr<T: type>;
extern macro MakeWeak(HeapObject): WeakHeapObject;
extern macro GetHeapObjectAssumeWeak(MaybeObject): HeapObject labels IfCleared;
macro StrongToWeak<T: type>(x: T): Weak<T> {
  return %RawDownCast<Weak<T>>(MakeWeak(x));
}
macro WeakToStrong<T: type>(x: Weak<T>): T labels ClearedWeakPointer {
  const x = GetHeapObjectAssumeWeak(x) otherwise ClearedWeakPointer;
  return %RawDownCast<T>(x);
}
macro Tag<T: type>(value: T): Smi {
  return %RawDownCast<Smi>(value);
}
macro Deep<T: type>(x: T): RawPtr<RawPtr<T>> {
  return %RawDownCast<RawPtr<RawPtr<T>>>(x);
}
"#;
    let file = compile_one("memory://nested-generic.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("Expected '>'")
            || item.contains("MakeWeak")
            || item.contains("matching callable")),
        "{messages:?}"
    );
}

#[test]
fn otherwise_return_is_a_statement_not_an_identifier() {
    let source = r#"
extern macro BranchIfNumberEqual(Number, Number): never
    labels Taken, NotTaken;
operator '==' macro IsNumberEqual(a: Number, b: Number): bool {
  BranchIfNumberEqual(a, b) otherwise return true, return false;
}
macro IsForceSlowPath(): bool {
  BranchIfNumberEqual(0, 1) otherwise return true;
  return false;
}
macro EarlyOut(x: Number): void {
  BranchIfNumberEqual(x, 0) otherwise return;
}
"#;
    let file = compile_one("memory://otherwise-return.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("Cannot resolve 'return'") || item.contains("Expected ';'")),
        "{messages:?}"
    );
}

#[test]
fn if_annotations_do_not_break_struct_fields_or_statements() {
    let source = r#"
struct float64_or_undefined_or_hole {
  @if(V8_ENABLE_UNDEFINED_DOUBLE)
  macro Value(): float64 labels IfUndefined, IfHole {
    if (this.is_undefined) {
      goto IfUndefined;
    }
    return this.value;
  }

  macro ValueUnsafeAssumeNotHole(): float64 {
    @if(V8_ENABLE_UNDEFINED_DOUBLE) {
      dcheck(!this.is_undefined);
    }
    return this.value;
  }

  @if(V8_ENABLE_UNDEFINED_DOUBLE) is_undefined: bool;
  is_hole: bool;
  value: float64;
}
macro Read(x: float64_or_undefined_or_hole): float64 {
  return x.value;
}
"#;
    let file = compile_one("memory://if-ann.tq", source.trim());
    assert_clean(&file);
}

#[test]
fn try_labels_are_visible_to_otherwise_clauses() {
    let source = r#"
transitioning builtin FastCreate(receiver: JSAny, value: JSAny): JSAny {
  try {
    const n = Cast<Smi>(receiver) otherwise Slow;
    if (n < 0) goto Slow;
    return n;
  } label Slow {
    return value;
  }
}
"#;
    let file = compile_one("memory://try-slow.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("Slow") || item.contains("Cannot resolve label")),
        "{messages:?}"
    );
}

#[test]
fn generic_struct_literals_match_applied_return_types() {
    let source = r#"
struct ConstantIterator<T: type> {
  value: T;
}
struct Slice<T: type, R: type> {
  start: T;
}
macro ConstantIterator<T: type>(value: T): ConstantIterator<T> {
  return ConstantIterator{value};
}
macro MakeSlice<T: type>(start: T): Slice<T, T> {
  return Slice<T, T>{start};
}
"#;
    let file = compile_one("memory://const-iter.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("not assignable")
            || item.contains("Cannot resolve type 'T'")),
        "{messages:?}"
    );
}

#[test]
fn generic_struct_methods_see_struct_type_parameters() {
    let source = r#"
struct ConstantIterator<T: type> {
  macro Next(): T labels _NoMore {
    return this.value;
  }
  value: T;
}
"#;
    let file = compile_one("memory://const-iter.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("not assignable")
            || item.contains("Cannot resolve type 'T'")),
        "{messages:?}"
    );
}

#[test]
fn intptr_converts_to_uintptr_through_unsigned() {
    let source = r#"
extern operator '+' macro ConstexprUintPtrAdd(
    constexpr uintptr, constexpr uintptr): constexpr intptr;
extern operator '+' macro UintPtrAdd(uintptr, uintptr): uintptr;
extern macro Unsigned(intptr): uintptr;
macro ConvertRelativeIndex(indexIntPtr: intptr, length: uintptr): uintptr {
  const relativeIndex: uintptr = Unsigned(indexIntPtr) + length;
  return relativeIndex;
}
"#;
    let file = compile_one("memory://unsigned.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn javascript_rest_arguments_expose_length_and_indexing() {
    let source = r#"
transitioning javascript builtin ArrayPrototypeConcat(
    js-implicit context: NativeContext, receiver: JSAny)(...arguments): JSAny {
  if (arguments.length == 0) {
    return receiver;
  }
  return arguments[0];
}
"#;
    let file = compile_one("memory://arguments.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("has no field")
            || item.contains("not assignable")
            || item.contains("Cannot resolve")),
        "{messages:?}"
    );
}

#[test]
fn bare_generic_specializations_are_callables() {
    let source = r#"
extern class PublicSymbol extends HeapObject {}
macro Cast<T: type>(o: HeapObject): T labels CastError;
Cast<PublicSymbol>(o: HeapObject): PublicSymbol labels CastError {
  return %RawDownCast<PublicSymbol>(o);
}
transitioning LoadJoinElement<Smi>(
    context: Context, receiver: JSReceiver, k: uintptr): JSAny {
  return receiver;
}
macro Main(o: HeapObject): PublicSymbol {
  return Cast<PublicSymbol>(o) otherwise unreachable;
}
macro FromConstexpr<To: type, From: type>(o: From): To;
FromConstexpr<intptr, constexpr IntegerLiteral>(i: constexpr IntegerLiteral):
    intptr {
  return Convert<intptr>(i);
}
"#;
    let file = compile_one("memory://specialize.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("Expected ':'")
            || item.contains("Expected callable")
            || item.contains("Cannot find matching callable 'Cast'")
            || item.contains("Cannot resolve type")),
        "{messages:?}"
    );
}

#[test]
fn indexed_class_fields_can_be_stored() {
    let source = r#"
extern class FixedArray extends HeapObject {
  objects[length]: Object;
}
macro Store(elements: FixedArray, index: Smi, value: Smi): void {
  elements[index] = value;
}
"#;
    let file = compile_one("memory://indexed.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn string_literals_and_enum_entries_match_throw_type_error() {
    let source = r#"
extern enum MessageTemplate { kIncompatibleMethodReceiver, ... }
extern macro ThrowTypeError(
    implicit context: Context)(constexpr MessageTemplate, Object, Object): never;
javascript builtin Foo(js-implicit context: NativeContext, receiver: JSAny)(): JSAny {
  ThrowTypeError(
      MessageTemplate::kIncompatibleMethodReceiver, 'get Foo', receiver);
  return receiver;
}
"#;
    let file = compile_one("memory://throw.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("ThrowTypeError") || item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn equal_wrapper_prefers_boolean_overload() {
    let source = r#"
extern macro Equal(JSAny, JSAny, Context): Boolean;
builtin Equal(implicit context: Context)(left: JSAny, right: JSAny): Object {
  return left;
}
macro WrapEqual(implicit context: Context)(left: JSAny, right: JSAny): Boolean {
  return Equal(left, right);
}
"#;
    let file = compile_one("memory://equal.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn optional_class_fields_parse_without_expected_colon() {
    let source = r#"
extern class ScopeInfo extends HeapObject {
  const flags: Smi;
  const module_variable_count?
      [flags == 1]: Smi;
  inferred_function_name?[flags == 2]: String|Undefined;
  outer_scope_info?: ScopeInfo;
}
macro Read(info: ScopeInfo): Smi {
  return info.flags;
}
"#;
    let file = compile_one("memory://optional-fields.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("Expected ':'")
            || item.contains("Expected '>'")
            || item.contains("Cannot resolve type")),
        "{messages:?}"
    );
}

#[test]
fn union_type_args_on_bare_specializations_parse() {
    let source = r#"
type TheHole;
macro Cast<T: type>(o: Object): T labels CastError;
Cast<JSAny|TheHole>(o: Object): JSAny|TheHole labels CastError {
  return %RawDownCast<JSAny|TheHole>(o);
}
macro Main(o: Object): JSAny|TheHole labels CastError {
  return Cast<JSAny|TheHole>(o) otherwise CastError;
}
"#;
    let file = compile_one("memory://union-specialization.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("Expected '>'")
            || item.contains("Cannot resolve type")
            || item.contains("Expected callable")),
        "{messages:?}"
    );
}

#[test]
fn generic_type_extends_slice_exposes_length() {
    let source = r#"
struct Slice<T: type, Reference: type> {
  const object: HeapObject;
  const offset: intptr;
  const length: intptr;
}
type MutableSlice<T: type> extends Slice<T, &T>;
extern class FixedArray extends HeapObject {
  objects[length]: Object;
}
macro SliceLength(slice: MutableSlice<Object>): intptr {
  return slice.length;
}
macro FromIndexed(a: FixedArray): intptr {
  const slice: MutableSlice<Object> = &a.objects;
  return slice.length;
}
"#;
    let file = compile_one("memory://slice-alias.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("has no field")
            || item.contains("Cannot resolve type")
            || item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn unknown_cpp_parent_types_are_not_errors() {
    let source = r#"
type ManagedWasmNativeModule extends CppGCManagedBase
    generates 'Tagged<Managed<wasm::NativeModule>>';
extern class JSBreakIterator extends JSObject {
  icu_break_iterator: CppGCManagedBase;
}
"#;
    let file = compile_one("memory://cppgc.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("Cannot resolve type 'CppGCManagedBase'")),
        "{messages:?}"
    );
}

#[test]
fn constexpr_string_variables_match_object_and_string_params() {
    let source = r#"
type string constexpr 'const char*';
extern enum MessageTemplate { kIncompatibleMethodReceiver, ... }
extern macro ThrowTypeError(
    implicit context: Context)(constexpr MessageTemplate, Object, Object): never;
extern macro ToThisString(implicit context: Context)(JSAny, String): String;
macro Main(implicit context: Context)(receiver: JSAny): String {
  const methodName: constexpr string = 'get Foo';
  ThrowTypeError(
      MessageTemplate::kIncompatibleMethodReceiver, methodName, receiver);
  return ToThisString(receiver, methodName);
}
"#;
    let file = compile_one("memory://constexpr-string.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("ThrowTypeError")
            || item.contains("ToThisString")
            || item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn generic_context_slot_fields_are_not_false_positives() {
    let source = r#"
extern class Context extends HeapObject {
  elements[length]: Object;
}
type Slot<Container: type, T: type> extends intptr;
macro InitContextSlot<
    ArgumentContext: type, AnnotatedContext: type, T: type, U: type>(
    context: ArgumentContext, index: Slot<AnnotatedContext, T>,
    value: U): void {
  const context: AnnotatedContext = context;
  const value: T = value;
  context.elements[index] = value;
}
"#;
    let file = compile_one("memory://generic-slot.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("has no field") || item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn deref_of_native_context_slot_is_the_slot_type() {
    let source = r#"
type Slot<Container: type, T: type> extends intptr;
extern enum ContextSlot extends intptr {
  PROMISE_FUNCTION_INDEX: Slot<NativeContext, JSFunction>,
}
macro NativeContextSlot<C: type, T: type>(
    implicit context: C)(index: Slot<NativeContext, T>):&T {
  return %RawDownCast<&T>(index);
}
macro Main(implicit context: Context)(): JSFunction {
  return *NativeContextSlot(ContextSlot::PROMISE_FUNCTION_INDEX);
}
"#;
    let file = compile_one("memory://deref-slot.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("not assignable") || item.contains("NativeContextSlot")),
        "{messages:?}"
    );
}

#[test]
fn function_pointers_and_builtin_aliases_typecheck() {
    let source = r#"
type BuiltinPtr extends Smi generates 'BuiltinPtr';
type ObjectToObject = builtin(Context, JSAny) => JSAny;
builtin TestHelperPlus1(x: Smi): Smi {
  return x;
}
macro TestFunctionPointers(): Smi {
  let fptr: builtin(Smi) => Smi = TestHelperPlus1;
  return fptr(42);
}
macro TestTypeAlias(x: ObjectToObject): BuiltinPtr {
  return x;
}
"#;
    let file = compile_one("memory://fnptr.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("fptr")
            || item.contains("not assignable")
            || item.contains("BuiltinPtr")),
        "{messages:?}"
    );
}

#[test]
fn bitfield_structs_convert_to_their_parent_word() {
    let source = r#"
extern macro Signed(uint32): int32;
bitfield struct Flags extends uint32 {
  a: bool: 1 bit;
  b: uint32: 8 bit;
}
macro Main(f: Flags): int32 {
  return Signed(f);
}
"#;
    let file = compile_one("memory://bitfield-signed.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages
            .iter()
            .any(|item| item.contains("Signed") || item.contains("not assignable")),
        "{messages:?}"
    );
}

#[test]
fn smi_tagged_bitfields_expose_flag_fields() {
    let source = r#"
@useParentTypeChecker type SmiTagged<T: type extends uint31> extends Smi;
bitfield struct JSPromiseFlags extends uint31 {
  status: uint32: 2 bit;
  has_handler: bool: 1 bit;
}
extern class JSPromise extends JSObject {
  flags: SmiTagged<JSPromiseFlags>;
}
macro Status(p: JSPromise): uint32 {
  return p.flags.status;
}
"#;
    let file = compile_one("memory://smi-tagged.tq", source.trim());
    let messages = messages(&file);
    assert!(
        !messages.iter().any(|item| item.contains("has no field")),
        "{messages:?}"
    );
}

fn compile_v8_tree() -> Option<torque_compiler::CompileResult> {
    let root = std::path::Path::new("/tmp/v8-full-tq");
    if !root.exists() {
        return None;
    }
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, files: &mut Vec<SourceFileInput>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("tq") {
                files.push(SourceFileInput {
                    uri: path.to_string_lossy().into_owned(),
                    text: std::fs::read_to_string(&path).unwrap_or_default(),
                });
            }
        }
    }
    walk(root, &mut files);
    Some(compile(&files))
}

#[test]
fn dump_real_workspace_has_no_diagnostics() {
    let Some(result) = compile_v8_tree() else {
        return;
    };
    let mut leftovers = Vec::new();
    for file in &result.files {
        for diagnostic in &file.diagnostics {
            leftovers.push(format!(
                "{}: {}",
                file.uri.rsplit('/').next().unwrap_or(&file.uri),
                diagnostic.message
            ));
        }
    }
    assert!(
        leftovers.is_empty(),
        "remaining diagnostics ({}) {:?}",
        leftovers.len(),
        leftovers.iter().take(40).collect::<Vec<_>>()
    );
}

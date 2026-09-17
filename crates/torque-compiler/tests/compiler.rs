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

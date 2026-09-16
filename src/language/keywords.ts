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

/** Keywords taken from V8's Torque grammar (src/torque) and the Torque user manual. */
export const TORQUE_KEYWORDS = [
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
] as const;

export const TORQUE_KEYWORD_SET: ReadonlySet<string> = new Set(TORQUE_KEYWORDS);

export const TORQUE_KEYWORD_DOCS: Readonly<Record<string, string>> = {
  bitfield: "Packed numeric fields stored inside a single integer value.",
  builtin: "A callable compiled as a V8 builtin (not inlined at the call site).",
  class: "A GC-heap object layout. `extern class` maps onto a C++ HeapObject subclass.",
  constexpr: "Evaluated at mksnapshot time, not when the generated builtin runs.",
  enum: "A named set of constants, typically matching a C++ enum.",
  extern: "The implementation or type lives in C++ / CSA, not in this Torque file.",
  generates: "Names the CSA TNode type produced for an abstract Torque type.",
  implicit: "An implicit parameter, commonly `context: Context`.",
  "js-implicit": "JavaScript-linkage implicit parameters such as context and receiver.",
  javascript: "Marks a builtin that uses JavaScript calling convention.",
  labels: "Exceptional exits that map to CSA labels.",
  macro: "Inlined CSA helper. `extern macro` binds to hand-written CSA.",
  namespace: "Declaration scope, analogous to a C++ namespace. Namespaces can be reopened.",
  never: "Return type for callables that only leave through labels or otherwise do not return.",
  otherwise: "Label used when a falling Cast or similar operation fails.",
  runtime: "Callable that jumps into a C++ runtime function.",
  shape: "A point-in-time JSObject in-object property layout; not a real instance type.",
  struct: "Pass-by-value aggregate. Unlike classes, structs can be generic.",
  tail: "Tail-call another callable.",
  transient: "Type that can be invalidated when object layout changes at runtime.",
  transitioning: "Operation that may change heap layout; invalidates transient types.",
  typeswitch: "Switch on the dynamic type of a tagged value.",
  void: "Return type for callables that do not produce a value.",
  weak: "Custom weak reference field (distinct from MaybeObject tagging).",
};

export const TORQUE_ANNOTATIONS = [
  "@abstract",
  "@apiExposedInstanceTypeValue",
  "@doNotGenerateCppClass",
  "@export",
  "@generateBodyDescriptor",
  "@generatePrint",
  "@hasSameInstanceTypeAsParent",
  "@highestInstanceTypeWithinParentClassRange",
  "@if",
  "@ifnot",
  "@lowestInstanceTypeWithinParentClassRange",
  "@noVerifier",
  "@reserveBitsInInstanceType",
] as const;

export const TORQUE_BUILTINS = [
  {
    name: "Cast",
    detail: "Cast<T>(value) otherwise Label",
    documentation: "Checked downcast. Jumps to the `otherwise` label when the value is not a T.",
  },
  {
    name: "Convert",
    detail: "Convert<T>(value)",
    documentation:
      "Unchecked conversion between compatible Torque types, including constexpr mappings.",
  },
  {
    name: "FromConstexpr",
    detail: "FromConstexpr<T>(value)",
    documentation: "Lowers a constexpr value to a runtime TNode of type T.",
  },
  {
    name: "UnsafeCast",
    detail: "UnsafeCast<T>(value)",
    documentation: "Unchecked cast. The caller must already know the value is a T.",
  },
  {
    name: "assert",
    detail: "assert value",
    documentation: "Debug-only check that the condition holds.",
  },
  {
    name: "check",
    detail: "check value",
    documentation: "Always-on check used for security or spec invariants.",
  },
  {
    name: "debug",
    detail: "debug",
    documentation: "Unconditional debug break in the generated CSA.",
  },
  {
    name: "unreachable",
    detail: "unreachable",
    documentation: "Marks a path the compiler should treat as impossible (`never`).",
  },
  {
    name: "Print",
    detail: "Print(value)",
    documentation: "Prints a value from generated CSA, useful in tests.",
  },
  {
    name: "static_assert",
    detail: "static_assert(condition)",
    documentation: "Checked while running mksnapshot, not at JS runtime.",
  },
] as const;

export const TORQUE_COMMON_TYPES = [
  "Boolean",
  "Context",
  "FixedArray",
  "HeapNumber",
  "HeapObject",
  "JSAny",
  "JSObject",
  "JSReceiver",
  "Map",
  "Name",
  "NativeContext",
  "Never",
  "Number",
  "Numeric",
  "Object",
  "Oddball",
  "Smi",
  "String",
  "Undefined",
  "bint",
  "bool",
  "float64",
  "float64_or_hole",
  "int31",
  "int32",
  "intptr",
  "never",
  "uint32",
  "uintptr",
  "void",
] as const;

export const TORQUE_SNIPPETS = [
  {
    label: "macro",
    insertText: "macro ${1:Name}(${2:arg}: ${3:Object}): ${4:void} {\n  $0\n}",
    documentation: "Define an inlined Torque macro.",
  },
  {
    label: "extern macro",
    insertText: "extern macro ${1:Name}(${2:arg}: ${3:Object}): ${4:void};",
    documentation: "Bind a hand-written CSA macro.",
  },
  {
    label: "builtin",
    insertText:
      "builtin ${1:Name}(implicit context: Context)(${2:arg}: ${3:Object}): ${4:Object} {\n  $0\n}",
    documentation: "Define a Torque builtin with CSA linkage.",
  },
  {
    label: "javascript builtin",
    insertText:
      "javascript builtin ${1:Name}(js-implicit context: NativeContext, receiver: JSAny)(${2:arg}: JSAny): ${3:JSAny} {\n  $0\n}",
    documentation: "Define a JavaScript-linkage builtin.",
  },
  {
    label: "class",
    insertText: "extern class ${1:Name} extends ${2:HeapObject} {\n  $0\n}",
    documentation: "Declare a heap object class layout.",
  },
  {
    label: "struct",
    insertText: "struct ${1:Name} {\n  ${2:field}: ${3:Object};\n  $0\n}",
    documentation: "Declare a pass-by-value struct.",
  },
  {
    label: "namespace",
    insertText: "namespace ${1:name} {\n  $0\n}",
    documentation: "Open or reopen a Torque namespace.",
  },
  {
    label: "typeswitch",
    insertText: "typeswitch (${1:value}) {\n  case (${2:x}: ${3:Smi}): {\n    $0\n  }\n}",
    documentation: "Switch on the dynamic type of a tagged value.",
  },
  {
    label: "type",
    insertText: "type ${1:Name} extends ${2:Object} generates '${3:TNode<Object>}';",
    documentation: "Declare an abstract Torque type.",
  },
  {
    label: "include",
    insertText: '#include "${1:path}"',
    documentation: "Include a C++ header from Torque.",
  },
] as const;

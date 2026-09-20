import { describe, expect, test } from "bun:test";
import { resolveDefinition } from "./analyze";
import { compileSources, lastCompileInputFileCount, lastCompileParseCount } from "./wasm";
import { TorqueWorkspace } from "./workspace";

describe("TorqueWorkspace", () => {
  test("rebuilds every file together and jumps to the other file's declaration", () => {
    const store = new TorqueWorkspace();
    const helper = "macro Helper(x: Smi): Smi { return x; }";
    const main = "macro Main(x: Smi): Smi { return Helper(x); }";
    store.load("memory://helper.tq", helper);
    store.set("memory://main.tq", main);
    const analysis = store.get("memory://main.tq");
    expect(analysis).toBeDefined();
    const offset = main.indexOf("Helper");
    const [symbol] = resolveDefinition(analysis!, offset, store.all());
    expect(symbol?.kind).toBe("macro");
    expect(symbol?.name).toBe("Helper");
    expect(symbol?.start).toBe(helper.indexOf("Helper"));
    const hit = analysis!.definitions.find(
      (item) => offset >= item.fromStart && offset <= item.fromEnd,
    );
    expect(hit?.toUri).toBe("memory://helper.tq");
  });

  test("jumps from a declaration name to that same declaration", () => {
    const store = new TorqueWorkspace();
    const helper = "macro Helper(x: Smi): Smi { return x; }";
    store.set("memory://helper.tq", helper);
    const analysis = store.get("memory://helper.tq");
    const [symbol] = resolveDefinition(analysis!, helper.indexOf("Helper"), store.all());
    expect(symbol?.kind).toBe("macro");
    expect(symbol?.name).toBe("Helper");
    expect(symbol?.start).toBe(helper.indexOf("Helper"));
  });

  test("jumps to a method declared in another file", () => {
    const store = new TorqueWorkspace();
    const helper = `
struct FastJSArrayForReadWitness {
  macro Recheck(): void labels CastError {}
}
`.trim();
    const main = `
macro Flatten(w: FastJSArrayForReadWitness): void labels CastError {
  w.Recheck() otherwise goto CastError;
}
`.trim();
    store.load("memory://helper.tq", helper);
    store.set("memory://main.tq", main);
    const analysis = store.get("memory://main.tq");
    const offset = main.indexOf("Recheck");
    const [symbol] = resolveDefinition(analysis!, offset, store.all());
    expect(symbol?.name).toBe("Recheck");
    expect(symbol?.start).toBe(helper.indexOf("Recheck"));
    const hit = analysis!.definitions.find(
      (item) => offset >= item.fromStart && offset <= item.fromEnd,
    );
    expect(hit?.toUri).toBe("memory://helper.tq");
  });

  test("reuses the parse of an unchanged file and does not recompile on ensure", () => {
    const store = new TorqueWorkspace();
    store.load("memory://parse-reuse-keep.tq", "macro Keep(): void {}");
    store.load("memory://parse-reuse-edit.tq", "macro Edit(): void {}");
    store.rebuild();
    expect(lastCompileParseCount()).toBe(2);
    expect(store.isDirty()).toBe(false);

    store.ensure("memory://parse-reuse-keep.tq", "macro Keep(): void {}");
    expect(store.isDirty()).toBe(false);
    expect(lastCompileParseCount()).toBe(2);

    store.load("memory://parse-reuse-edit.tq", "macro Edit(): void {");
    store.ensure("memory://parse-reuse-edit.tq", "macro Edit(): void {");
    expect(lastCompileParseCount()).toBe(2);
    expect(
      store
        .get("memory://parse-reuse-edit.tq")
        ?.diagnostics.some((item) => item.message.includes("Unclosed")),
    ).toBe(false);

    store.rebuild();
    expect(lastCompileParseCount()).toBe(1);
    expect(
      store
        .get("memory://parse-reuse-edit.tq")
        ?.diagnostics.some((item) => item.message.includes("Unclosed")),
    ).toBe(true);
    expect(
      store
        .get("memory://parse-reuse-keep.tq")
        ?.diagnostics.some((item) => item.message.includes("Unclosed")),
    ).toBe(false);
  });

  test("refresh reanalyzes only the edited file and leaves the sibling analysis in place", () => {
    const store = new TorqueWorkspace();
    store.load("memory://refresh-keep.tq", "macro Keep(): void {}");
    store.load("memory://refresh-edit.tq", "macro Edit(): void {}");
    store.rebuild();
    const kept = store.get("memory://refresh-keep.tq");
    store.load("memory://refresh-edit.tq", "macro Edit(): void {\n");
    const edited = store.refresh("memory://refresh-edit.tq");
    expect(lastCompileParseCount()).toBe(1);
    expect(store.get("memory://refresh-keep.tq")).toBe(kept);
    expect(edited.diagnostics.some((item) => item.message.includes("Unclosed"))).toBe(true);
    expect(
      store
        .get("memory://refresh-keep.tq")
        ?.diagnostics.some((item) => item.message.includes("Unclosed")),
    ).toBe(false);
    const count = lastCompileParseCount();
    expect(store.refresh("memory://refresh-edit.tq")).toBe(edited);
    expect(lastCompileParseCount()).toBe(count);
  });

  test("refresh does not report ElementsKind enum entries declared in a sibling file", () => {
    const store = new TorqueWorkspace();
    store.load("memory://elements-kind.tq", "extern enum ElementsKind { PACKED_SMI_ELEMENTS }");
    store.load(
      "memory://array-filter.tq",
      "macro FastFilter(): JSReceiver { return AllocateJSArray(ElementsKind::PACKED_SMI_ELEMENTS); }",
    );
    store.rebuild();
    store.load(
      "memory://array-filter.tq",
      "macro FastFilter(): JSReceiver {\n  return AllocateJSArray(ElementsKind::PACKED_SMI_ELEMENTS);\n}",
    );
    const analysis = store.refresh("memory://array-filter.tq");
    expect(
      analysis.diagnostics.some((item) => item.message === "Cannot resolve 'PACKED_SMI_ELEMENTS'"),
    ).toBe(false);
  });

  test("refresh does not flood errors for a sibling constexpr used after a one-character edit", () => {
    const store = new TorqueWorkspace();
    store.load(
      "memory://base.tq",
      "const kMaxNewSpaceFixedArrayElements: constexpr int31 generates 'FixedArray::kMaxRegularLength';",
    );
    store.load(
      "memory://array-join.tq",
      "const kMaxBufferChunkSize: constexpr int31 = kMaxNewSpaceFixedArrayElements;",
    );
    store.rebuild();
    store.load(
      "memory://array-join.tq",
      "const kMaxBufferChunkSize: constexpr int31 = kMaxNewSpaceFixedArrayElements",
    );
    const analysis = store.refresh("memory://array-join.tq");
    expect(
      analysis.diagnostics.some(
        (item) => item.message === "Cannot resolve 'kMaxNewSpaceFixedArrayElements'",
      ),
    ).toBe(false);
    expect(analysis.diagnostics.some((item) => item.message === "Expected ';'")).toBe(true);
  });

  test("refresh does not emit array-join false positives when siblings declare the names", () => {
    const store = new TorqueWorkspace();
    const objects = `
extern class HeapObject {
  const map: Map;
}
extern class PrimitiveHeapObject extends HeapObject {}
extern class Name extends PrimitiveHeapObject {}
extern class String extends Name {}
extern operator '.length_intptr' macro LoadStringLengthAsWord(String): intptr;
extern class Map {
  elements_kind: ElementsKind;
}
extern enum ElementsKind { PACKED_SMI_ELEMENTS }
extern class JSReceiver extends HeapObject {}
extern class Boolean extends PrimitiveHeapObject {
  to_string: String;
}
extern class Null extends PrimitiveHeapObject {}
type TheHole;
extern macro TheHoleConstant(): TheHole;
const TheHole: TheHole = TheHoleConstant();
`.trim();
    const join = `
macro ArrayPrototypeJoinImpl(array: JSReceiver, len: Number, separator: String): String {
  return separator;
}
macro CycleProtectedArrayJoin<T: type>(
    toLocale: bool, array: T, len: Number, separator: String, locales: JSAny, options: JSAny): String {
  return separator;
}
macro UseJoinNames(
    array: JSReceiver, str: String, m: Map, b: Boolean, rec: JSReceiver, n: Null,
    element: Object, count: Number): String {
  const hole = element == TheHole ? str : str;
  const len = str.length_intptr;
  const asPrimitive: PrimitiveHeapObject = str;
  const strMap = str.map;
  const asNull: PrimitiveHeapObject = n;
  const ts = b.to_string;
  const recMap = rec.map;
  const kind = m.elements_kind;
  const joined = ArrayPrototypeJoinImpl(array, count, str);
  return CycleProtectedArrayJoin<JSReceiver>(false, rec, count, str, Undefined, Undefined);
}
`.trim();
    store.load("memory://objects.tq", objects);
    store.load("memory://array-join.tq", join);
    store.rebuild();
    const kept = store.get("memory://objects.tq");
    store.load("memory://array-join.tq", `${join}\n`);
    const analysis = store.refresh("memory://array-join.tq");
    expect(store.get("memory://objects.tq")).toBe(kept);
    expect(lastCompileParseCount()).toBe(1);
    expect(lastCompileInputFileCount()).toBe(1);
    const forbidden = [
      "Cannot resolve 'TheHole'",
      "Type 'String' has no field 'length_intptr'",
      "Type 'String' is not assignable to 'PrimitiveHeapObject'",
      "Type 'String' has no field 'map'",
      "Type 'Null' is not assignable to 'PrimitiveHeapObject'",
      "Type 'Boolean' has no field 'to_string'",
      "Type 'JSReceiver' has no field 'map'",
      "Type 'Map' has no field 'elements_kind'",
      "Cannot find matching callable 'ArrayPrototypeJoinImpl'",
      "Cannot find matching callable 'CycleProtectedArrayJoin'",
    ];
    const messages = analysis.diagnostics.map((item) => item.message);
    expect(forbidden.filter((item) => messages.includes(item))).toEqual([]);
  });

  test("refresh still reports a misspelled sibling constexpr", () => {
    const store = new TorqueWorkspace();
    store.load(
      "memory://base.tq",
      "const kMaxNewSpaceFixedArrayElements: constexpr int31 generates 'FixedArray::kMaxRegularLength';",
    );
    store.load(
      "memory://array-join.tq",
      "const kMaxBufferChunkSize: constexpr int31 = kMaxNewSpaceFixedArrayElement;",
    );
    store.rebuild();
    store.load(
      "memory://array-join.tq",
      "const kMaxBufferChunkSize: constexpr int31 = kMaxNewSpaceFixedArrayElement;\n",
    );
    const analysis = store.refresh("memory://array-join.tq");
    expect(
      analysis.diagnostics.some(
        (item) => item.message === "Cannot resolve 'kMaxNewSpaceFixedArrayElement'",
      ),
    ).toBe(true);
  });
});

describe("compileSources", () => {
  test("returns compiler diagnostics and definition mappings through WASM", () => {
    const files = compileSources([
      {
        uri: "memory://wrong.tq",
        text: "macro Wrong(x: Smi): String { return x; }",
      },
    ]);
    expect(files).toHaveLength(1);
    expect(files[0]?.diagnostics.some((item) => item.message.includes("not assignable"))).toBe(
      true,
    );
    const mapped = compileSources([
      {
        uri: "memory://sample.tq",
        text: "macro Helper(x: Smi): Smi { return x; }\nmacro Main(x: Smi): Smi { return Helper(x); }",
      },
    ]);
    const text =
      "macro Helper(x: Smi): Smi { return x; }\nmacro Main(x: Smi): Smi { return Helper(x); }";
    const from = text.lastIndexOf("Helper");
    const hit = mapped[0]?.definitions.find(
      (item) => from >= item.fromStart && from <= item.fromEnd,
    );
    expect(hit?.toStart).toBe(text.indexOf("Helper"));
  });
});

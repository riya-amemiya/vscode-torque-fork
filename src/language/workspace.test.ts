import { describe, expect, test } from "bun:test";
import { resolveDefinition } from "./analyze";
import { compileSources, lastCompileParseCount } from "./wasm";
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

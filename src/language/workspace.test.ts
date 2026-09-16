import { describe, expect, test } from "bun:test";
import { resolveDefinition } from "./analyze";
import { compileSources } from "./wasm";
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

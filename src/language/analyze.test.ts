import { describe, expect, test } from "bun:test";
import { analyzeDocument, includeAt, resolveDefinition } from "./analyze";
import { completionsFor } from "./complete";

const sample = `
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
`.trim();

describe("analyzeDocument", () => {
  test("extracts namespaces, types, classes, fields, and builtins", () => {
    const analysis = analyzeDocument(sample);
    const names = analysis.symbols.map((symbol) => `${symbol.kind}:${symbol.name}`);
    expect(names).toContain("namespace:math");
    expect(names).toContain("type:Number");
    expect(names).toContain("class:JSProxy");
    expect(names).toContain("field:target");
    expect(names).toContain("builtin:HeapNumberIs42");
    expect(names).toContain("builtin:MathIs42");
    expect(names).toContain("const:number");
    expect(names).not.toContain("field:True");
  });

  test("detects unmatched braces as syntax errors", () => {
    const analysis = analyzeDocument("macro Broken(): void { if (true) {");
    expect(analysis.diagnostics.some((item) => item.message.includes("Unclosed"))).toBe(true);
  });

  test("detects a missing macro name", () => {
    const analysis = analyzeDocument("macro (): void {}");
    expect(analysis.diagnostics.some((item) => item.message === "Expected macro name")).toBe(true);
  });

  test("records #include paths", () => {
    const analysis = analyzeDocument('#include "src/objects/js-proxy.h"\n');
    expect(analysis.includes).toEqual([
      { path: "src/objects/js-proxy.h", start: expect.any(Number), end: expect.any(Number) },
    ]);
    const offset = analysis.text.indexOf("js-proxy");
    expect(includeAt(analysis, offset)?.path).toBe("src/objects/js-proxy.h");
  });
});

describe("resolveDefinition", () => {
  test("jumps from a call to the builtin declaration", () => {
    const analysis = analyzeDocument(sample);
    const offset = sample.lastIndexOf("HeapNumberIs42");
    const [symbol] = resolveDefinition(analysis, offset, []);
    expect(symbol?.kind).toBe("builtin");
    expect(symbol?.name).toBe("HeapNumberIs42");
    expect(symbol?.start).toBe(sample.indexOf("HeapNumberIs42"));
  });
});

describe("completionsFor", () => {
  test("suggests typeswitch, Cast, and declared builtins", () => {
    const analysis = analyzeDocument(sample);
    const items = completionsFor(analysis, sample.length, []);
    const labels = items.map((item) => item.label);
    expect(labels).toContain("typeswitch");
    expect(labels).toContain("Cast");
    expect(labels).toContain("MathIs42");
    expect(
      items.some((item) => item.kind === "snippet" && item.label === "javascript builtin"),
    ).toBe(true);
  });

  test("filters by prefix and annotation trigger", () => {
    const analysis = analyzeDocument("@ex");
    const items = completionsFor(analysis, 3, []);
    expect(items.map((item) => item.label)).toContain("@export");
    expect(items.every((item) => item.label.startsWith("@"))).toBe(true);
  });
});

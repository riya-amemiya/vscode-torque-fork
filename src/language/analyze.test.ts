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

  test("does not treat runtime:: calls as runtime declarations", () => {
    const source = `
transitioning macro ArrayIsArray_Inline(
    implicit context: Context)(element: JSAny): Boolean {
  return Cast<Boolean>(runtime::ArrayIsArray(element)) otherwise unreachable;
}
`.trim();
    const analysis = analyzeDocument(source);
    expect(analysis.diagnostics.map((item) => item.message)).not.toContain("Expected runtime name");
    expect(analysis.symbols.map((symbol) => `${symbol.kind}:${symbol.name}`)).toContain(
      "macro:ArrayIsArray_Inline",
    );
  });

  test("records #include paths", () => {
    const analysis = analyzeDocument('#include "src/objects/js-proxy.h"\n');
    expect(analysis.includes).toEqual([
      { path: "src/objects/js-proxy.h", start: expect.any(Number), end: expect.any(Number) },
    ]);
    const offset = analysis.text.indexOf("js-proxy");
    expect(includeAt(analysis, offset)?.path).toBe("src/objects/js-proxy.h");
  });

  test("does not emit parser garbage for otherwise goto, this, or rest arguments", () => {
    const source = `
struct Vec {
  macro Recheck(): void labels CastError {}
  macro Store(implicit context: Context)(): JSAny {
    return this.fixedArray;
  }
  fixedArray: JSAny;
}
macro Flatten(implicit context: Context)(source: Vec, length: Smi): Vec labels Bailout {
  const empty: JSAny = length > 0 ? length : 0;
  source.Recheck() otherwise goto Bailout;
  return Vec{fixedArray: empty};
}
transitioning javascript builtin ArrayPrototypeFlat(
    js-implicit context: NativeContext, receiver: JSAny)(...arguments): JSAny {
  return arguments[0];
}
`.trim();
    const analysis = analyzeDocument(source);
    const garbage = [
      "Expected ';'",
      "Expected ')'",
      "Cannot resolve 'this'",
      "Cannot resolve 'goto'",
      "Cannot resolve 'arguments'",
      "Cannot resolve 'context'",
    ];
    expect(analysis.diagnostics.filter((item) => garbage.includes(item.message))).toEqual([]);
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

  test("reports type mismatches as compiler errors", () => {
    const analysis = analyzeDocument(
      `
macro Wrong(x: Smi): String {
  return x;
}
`.trim(),
    );
    expect(analysis.diagnostics.some((item) => item.message.includes("not assignable"))).toBe(true);
  });

  test("reports type-argument inference failures", () => {
    const analysis = analyzeDocument(
      `
macro Pick<T: type>(x: T, y: T): T { return x; }
macro Main(a: Smi, b: String): Smi {
  return Pick(a, b);
}
`.trim(),
    );
    expect(analysis.diagnostics.some((item) => item.message.includes("conflicting types"))).toBe(
      true,
    );
  });

  test("reports uninferable generic calls as compiler errors", () => {
    const analysis = analyzeDocument(
      `
macro Identity<T: type>(): T;
macro Main(): Smi {
  return Identity();
}
`.trim(),
    );
    expect(
      analysis.diagnostics.some((item) =>
        item.message.includes("failed to infer arguments for all type parameters"),
      ),
    ).toBe(true);
  });

  test("jumps from a local use to its declaration", () => {
    const source = `
macro Main(a: Smi): Smi {
  let b: Smi = a;
  return b;
}
`.trim();
    const analysis = analyzeDocument(source);
    const offset = source.lastIndexOf("b");
    const [symbol] = resolveDefinition(analysis, offset, []);
    expect(symbol?.name).toBe("b");
    expect(symbol?.start).toBe(source.indexOf("let b") + "let ".length);
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

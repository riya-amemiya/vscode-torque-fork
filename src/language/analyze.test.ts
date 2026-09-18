import { describe, expect, test } from "bun:test";
import { analyzeDocument, dropResolvedElsewhere, includeAt, resolveDefinition } from "./analyze";
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

  test("jumps from a declaration name to that declaration", () => {
    const source = "macro Helper(x: Smi): Smi { return x; }";
    const analysis = analyzeDocument(source);
    const [symbol] = resolveDefinition(analysis, source.indexOf("Helper"), []);
    expect(symbol?.kind).toBe("macro");
    expect(symbol?.name).toBe("Helper");
    expect(symbol?.start).toBe(source.indexOf("Helper"));
  });

  test("jumps to the parameter under the cursor when the name is reused", () => {
    const source = `
macro Helper(x: Smi): Smi { return x; }
macro Main(x: Smi): Smi { return Helper(x); }
`.trim();
    const analysis = analyzeDocument(source);
    const offset = source.indexOf("Main(x") + "Main(".length;
    const [symbol] = resolveDefinition(analysis, offset, []);
    expect(symbol?.name).toBe("x");
    expect(symbol?.start).toBe(offset);
    expect(symbol?.containerName).toBe("Main");
  });

  test("jumps to the macro under the cursor when the name is overloaded", () => {
    const source = `
macro Helper(x: Smi): Smi { return x; }
macro Helper(x: String): String { return x; }
`.trim();
    const analysis = analyzeDocument(source);
    const offset = source.lastIndexOf("Helper");
    const [symbol] = resolveDefinition(analysis, offset, []);
    expect(symbol?.name).toBe("Helper");
    expect(symbol?.start).toBe(offset);
  });

  test("jumps from a parameter name immediately after '('", () => {
    const source = "macro Helper(x: Smi): Smi { return x; }";
    const analysis = analyzeDocument(source);
    const offset = source.indexOf("x");
    const [symbol] = resolveDefinition(analysis, offset, []);
    expect(symbol?.kind).toBe("const");
    expect(symbol?.name).toBe("x");
    expect(symbol?.start).toBe(offset);
  });

  test("jumps from the exclusive end of a declaration name", () => {
    const source = "macro Helper(x: Smi): Smi { return x; }";
    const analysis = analyzeDocument(source);
    const start = source.indexOf("Helper");
    const [symbol] = resolveDefinition(analysis, start + "Helper".length, []);
    expect(symbol?.name).toBe("Helper");
    expect(symbol?.start).toBe(start);
  });

  test("jumps each generic T to its own callable, not a sibling T", () => {
    const source = `
macro Alpha<T : type extends Smi>(x: T): T {
  return Convert<T>(x);
}
macro Beta<T : type extends String>(y: T): T {
  return Convert<T>(y);
}
`.trim();
    const analysis = analyzeDocument(source);
    const alphaT = source.indexOf("<T :") + 1;
    const betaHeader = source.indexOf("macro Beta");
    const betaT = source.indexOf("<T :", betaHeader) + 1;
    const betaConvert = source.lastIndexOf("Convert<T>") + "Convert<".length;

    const [fromDecl] = resolveDefinition(analysis, betaT, []);
    expect(fromDecl?.name).toBe("T");
    expect(fromDecl?.kind).toBe("type");
    expect(fromDecl?.start).toBe(betaT);
    expect(fromDecl?.start).not.toBe(alphaT);

    const [fromUse] = resolveDefinition(analysis, betaConvert, []);
    expect(fromUse?.name).toBe("T");
    expect(fromUse?.kind).toBe("type");
    expect(fromUse?.start).toBe(betaT);
    expect(fromUse?.start).not.toBe(alphaT);
  });

  test("does not resolve Cast type arguments to a generic parameter", () => {
    const source = `
macro Alpha<T : type extends Smi>(x: T): T { return x; }
macro Main(receiver: JSAny): JSReceiver {
  return Cast<JSReceiver>(receiver) otherwise unreachable;
}
`.trim();
    const analysis = analyzeDocument(source);
    const jsReceiver = source.indexOf("Cast<JSReceiver>") + "Cast<".length;
    const resolved = resolveDefinition(analysis, jsReceiver, []);
    expect(resolved.every((symbol) => symbol.name !== "T")).toBe(true);
  });

  test("does not keep Cannot resolve for enum entries declared in another file", () => {
    const source = `
macro FastFilterSpeciesCreate(receiver: JSReceiver): JSReceiver {
  return AllocateJSArray(ElementsKind::PACKED_SMI_ELEMENTS, receiver);
}
`.trim();
    const analysis = analyzeDocument(source);
    expect(
      analysis.diagnostics.some((item) => item.message === "Cannot resolve 'PACKED_SMI_ELEMENTS'"),
    ).toBe(true);
    const enumFile = analyzeDocument(
      "extern enum ElementsKind { PACKED_SMI_ELEMENTS }",
      "memory://elements-kind.tq",
    );
    const filtered = dropResolvedElsewhere(analysis, [enumFile]);
    expect(
      filtered.diagnostics.some((item) => item.message === "Cannot resolve 'PACKED_SMI_ELEMENTS'"),
    ).toBe(false);
  });

  test("places Expected ';' after otherwise unreachable, not on the next const", () => {
    const source = `
macro Main(receiver: JSAny, callback: JSAny): void {
  const jsreceiver = Cast<JSReceiver>(receiver) otherwise unreachable
  const callbackfn = Cast<Callable>(callback) otherwise unreachable;
}
`.trim();
    const analysis = analyzeDocument(source);
    const secondConst = source.indexOf("const callbackfn");
    const secondConstEnd = secondConst + "const".length;
    const unreachable = source.indexOf("unreachable");
    const unreachableEnd = unreachable + "unreachable".length;
    const expectedSemi = analysis.diagnostics.filter((item) => item.message === "Expected ';'");
    expect(expectedSemi.length).toBeGreaterThan(0);
    expect(
      expectedSemi.every((item) => item.start >= secondConstEnd || item.end <= secondConst),
    ).toBe(true);
    expect(
      expectedSemi.every((item) => item.start >= unreachableEnd && item.start < secondConst),
    ).toBe(true);
  });

  test("falls back to workspace symbols when the compiler map misses", () => {
    const helper = "macro Helper(x: Smi): Smi { return x; }";
    const main = "macro Main(x: Smi): Smi { return Helper(x); }";
    const helperAnalysis = analyzeDocument(helper, "memory://helper.tq");
    const mainAnalysis = analyzeDocument(main, "memory://main.tq");
    mainAnalysis.definitions = [];
    const [symbol] = resolveDefinition(mainAnalysis, main.indexOf("Helper"), [helperAnalysis]);
    expect(symbol?.kind).toBe("macro");
    expect(symbol?.name).toBe("Helper");
    expect(symbol?.start).toBe(helper.indexOf("Helper"));
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

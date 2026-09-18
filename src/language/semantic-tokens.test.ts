import { describe, expect, test } from "bun:test";
import { analyzeDocument, type DocumentAnalysis } from "./analyze";
import { positionToOffset } from "./positions";
import { semanticTokensFor, type SemanticSpan } from "./semantic-tokens";

const source = `
macro Main(a: Smi): Smi labels Fail {
  const x: Smi = a;
  let y: Smi = x;
  goto Fail;
  return y;
}
`.trim();

function spansNamed(
  spans: SemanticSpan[],
  analysis: DocumentAnalysis,
  name: string,
): SemanticSpan[] {
  return spans.filter((span) => {
    const start = positionToOffset(analysis.lines, {
      line: span.line,
      character: span.character,
    });
    return analysis.text.slice(start, start + span.length) === name;
  });
}

describe("semanticTokensFor", () => {
  test("colors builtin types, const vs let, and labels", () => {
    const analysis = analyzeDocument(source);
    const spans = semanticTokensFor(analysis);
    const smi = spansNamed(spans, analysis, "Smi");
    expect(smi.length).toBeGreaterThan(0);
    expect(smi.every((span) => span.type === "type")).toBe(true);

    const x = spansNamed(spans, analysis, "x");
    expect(x.length).toBeGreaterThan(1);
    expect(x.every((span) => span.type === "variable" && span.modifiers.includes("readonly"))).toBe(
      true,
    );
    expect(x.some((span) => span.modifiers.includes("declaration"))).toBe(true);

    const y = spansNamed(spans, analysis, "y");
    expect(y.length).toBeGreaterThan(1);
    expect(y.every((span) => span.type === "variable")).toBe(true);
    expect(y.every((span) => !span.modifiers.includes("readonly"))).toBe(true);
    expect(y.some((span) => span.modifiers.includes("declaration"))).toBe(true);

    const fail = spansNamed(spans, analysis, "Fail");
    expect(fail.length).toBeGreaterThan(1);
    expect(fail.every((span) => span.type === "label")).toBe(true);
  });

  test("records let and label symbols from the compiler", () => {
    const analysis = analyzeDocument(source);
    const names = analysis.symbols.map((symbol) => `${symbol.kind}:${symbol.name}`);
    expect(names).toContain("const:x");
    expect(names).toContain("let:y");
    expect(names).toContain("label:Fail");
  });
});

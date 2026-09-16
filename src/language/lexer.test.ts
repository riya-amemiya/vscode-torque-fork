import { describe, expect, test } from "bun:test";
import { tokenize } from "./lexer";

describe("tokenize", () => {
  test("classifies Torque keywords, identifiers, and js-implicit", () => {
    const tokens = tokenize("javascript builtin Foo(js-implicit context: NativeContext)(): void");
    const kinds = tokens.map((token) => `${token.kind}:${token.text}`);
    expect(kinds).toContain("keyword:javascript");
    expect(kinds).toContain("keyword:builtin");
    expect(kinds).toContain("identifier:Foo");
    expect(kinds).toContain("keyword:js-implicit");
    expect(kinds).toContain("keyword:void");
  });

  test("reports unterminated strings and comments", () => {
    expect(tokenize('"hello').some((token) => token.kind === "error")).toBe(true);
    expect(
      tokenize("/* oops").some((token) => token.message === "Unterminated block comment"),
    ).toBe(true);
  });

  test("keeps #include as a directive token followed by a string", () => {
    const tokens = tokenize('#include "src/objects/js-proxy.h"');
    expect(tokens[0]).toMatchObject({ kind: "include", text: "#include" });
    expect(tokens[1]).toMatchObject({ kind: "string", text: '"src/objects/js-proxy.h"' });
  });
});

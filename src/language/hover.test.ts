import { describe, expect, test } from "bun:test";
import { analyzeDocument } from "./analyze";
import { hoverFor } from "./hover";

describe("hoverFor", () => {
  test("documents typeswitch and Cast", () => {
    const typeswitch = analyzeDocument("typeswitch (x) { case (smi: Smi): { } }");
    const keyword = hoverFor(typeswitch, typeswitch.text.indexOf("typeswitch") + 1);
    expect(keyword?.title).toBe("typeswitch");
    expect(keyword?.body).toContain("dynamic type");

    const cast = analyzeDocument("const x = Cast<Smi>(value) otherwise Fail;");
    const builtin = hoverFor(cast, cast.text.indexOf("Cast") + 1);
    expect(builtin?.title).toContain("Cast");
    expect(builtin?.body).toContain("otherwise");
  });
});

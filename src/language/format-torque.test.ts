import { describe, expect, test } from "bun:test";
import { formatTorque } from "./format-torque";

const messy = `namespace torque{
@export
macro Foo(x:int):int labels Fail{
  if constexpr(x){return x;}
  typeswitch (x) {
    case (smi:Smi): {return smi;}
    case (heap:HeapObject): deferred {return heap;}
  }
  return %Raw(x) otherwise Fail;
}
}
#include "src/builtins/base.tq"
`;

const formatted = `namespace torque {
@export
macro Foo(x: int): int labels Fail {
  if constexpr (x) {
    return x;
  }
  typeswitch (x) {
    case (smi: Smi): {
      return smi;
    }
    case (heap: HeapObject): deferred {
      return heap;
    }
  }
  return %Raw(x) otherwise Fail;
}
}
#include "src/builtins/base.tq"
`;

describe("formatTorque", () => {
  test("formats Torque the way format-torque.py does", () => {
    expect(formatTorque(messy)).toBe(formatted);
  });

  test("leaves a formatted file unchanged", () => {
    expect(formatTorque(formatted)).toBe(formatted);
  });

  test("formats classes, annotations, operators, and implicit parameters", () => {
    const source = `extern class Array extends HeapObject{a:int;}
struct Pair{a:int;b:int;}
@if(TAGGED){const x=1;}
@ifnot(SAND){const y=2;}
@export
extern macro Bar();
macro Foo(implicit context:Context)(x:int):int{return x;}
macro Baz(js-implicit context:NativeContext)():void{}
operator '+' (a:int,b:int):int{return a+b;}
macro Read(p:*int,q:&int):int{return *p;}
`;
    const result = formatTorque(source);
    expect(result).toContain("extern class Array extends HeapObject {\n  a: int;\n}");
    expect(result).toContain("struct Pair {\n  a: int;\n  b: int;\n}");
    expect(result).toContain("@if(TAGGED)");
    expect(result).toContain("@ifnot(SAND)");
    expect(result).toContain("@export\nextern macro Bar();");
    expect(result).toContain("macro Foo(implicit context: Context)(x: int): int");
    expect(result).toContain("macro Baz(js-implicit context: NativeContext)(): void {}");
    expect(result).toContain("operator '+'(a: int, b: int): int");
    expect(result).toContain("macro Read(p:*int, q:&int): int {\n  return *p;\n}");
    expect(formatTorque(result)).toBe(result);
  });
});

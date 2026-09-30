declare namespace WebAssembly {
  class Module {
    constructor(bytes: BufferSource);
  }

  class Memory {
    readonly buffer: ArrayBuffer;
  }

  class Instance {
    constructor(module: Module, importObject?: object);
    readonly exports: Record<string, unknown>;
  }
}

declare module "@wasm-fmt/clang-format/wasm" {
  const wasmPath: string;
  export default wasmPath;
}

declare module "torque-compiler/torque_compiler_bg.wasm" {
  const wasmPath: string;
  export default wasmPath;
}

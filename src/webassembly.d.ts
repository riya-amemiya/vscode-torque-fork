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

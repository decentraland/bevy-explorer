import init, * as wasm_bindgen_exports from "./pkg/webgpu_build.js";

console.log("[Asset Processor] Starting");

self.onmessage = async (event) => {
  if (event.data && event.data.type === "INIT_ASSET_PROCESSOR") {
    const { compiledModule, sharedMemory } = event.data.payload;

    if (!compiledModule || !sharedMemory) {
      console.error("[Asset Loader] Invalid payload received.");
      return;
    }

    try {
      // init wasm
      console.log("[Asset Processor] init wasm");
      await init({ module_or_path: compiledModule, memory: sharedMemory });
      console.log("[Asset Processor] init processor channels");
      wasm_bindgen_exports.image_processor_init();
      postMessage({ type: `INITIALIZED` });
      console.log("[Asset Processor] defer to asset process thread");
      await wasm_bindgen_exports.image_processor_run();
      console.log("[Asset Processor] exited?!");
    } catch (e) {
      console.error(
        "[Asset Processor] Error during Wasm instantiation or setup:",
        e
      );
    }
  }
};

postMessage({ type: `READY` });

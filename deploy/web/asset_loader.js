import init, * as wasm_bindgen_exports from "./pkg/webgpu_build.js";

console.log("[Asset Loader] Starting");

self.onmessage = async (event) => {
  if (event.data && event.data.type === "INIT_ASSET_LOADER") {
    const { compiledModule, sharedMemory } = event.data.payload;

    if (!compiledModule || !sharedMemory) {
      console.error("[Asset Loader] Invalid payload received.");
      return;
    }

    try {
      // init wasm
      console.log("[Asset Loader] init wasm");
      await init({ module: compiledModule, memory: sharedMemory });
      console.log("[Asset Loader] init asset load thread");
      await wasm_bindgen_exports.init_asset_load_thread();
      console.log("[Asset Loader] running");
      postMessage({ type: `INITIALIZED`});
    } catch (e) {
      console.error(
        "[Scene Worker] Error during Wasm instantiation or setup:",
        e
      );
    }
  }
};

postMessage({ type: `READY` });

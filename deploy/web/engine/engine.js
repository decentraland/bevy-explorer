import init, { engine_init, engine_run } from "../pkg/webgpu_build.js";

const canvas = document.getElementById("mygame-canvas");

var sharedMemory;

export async function initEngine() {
  const wasmUrl = "../pkg/webgpu_build_bg.wasm";

  try {
    const compiledModule = await WebAssembly.compileStreaming(fetch(wasmUrl));

    const initialMemoryPages = 640; // setting initial memory high causes malloc failures
    const maximumMemoryPages = 65536;
    const sharedMemory = new WebAssembly.Memory({
      initial: initialMemoryPages,
      maximum: maximumMemoryPages,
      shared: true,
    });
    window.wasm_memory = sharedMemory;

    // TODO: figure out why using blobs fails with livekit feature enabled
    /*
    const wasmJs = URL.createObjectURL(
      await fetch("../pkg/webgpu_build.js")
        .then((response) => response.text())
        .then((text) => new Blob([text], { type: "application/javascript" }))
    );

    const sandboxJs = URL.createObjectURL(
      await fetch("sandbox_worker.js")
        .then((response) => response.text())
        .then((text) => {
          const replacedText = text.replace("../pkg/webgpu_build.js", wasmJs);
          return new Blob([replacedText], { type: "application/javascript" });
        })
    );
    */

    window.spawn_and_init_sandbox = async () => {
      var timeoutId;
      return new Promise((resolve, _reject) => {
        // var sandboxWorker = new Worker(sandboxJs, { type: "module" });
        var sandboxWorker = new Worker(new URL("sandbox_worker.js", import.meta.url), { type: "module" });

        var timeoutCount = 0;
        let logTimeout = () => {
          console.log(
            "[Engine JS] Still waiting for worker to init",
            timeoutCount
          );
          timeoutCount += 1;
          timeoutId = setTimeout(logTimeout, 5000);
        };
        timeoutId = setTimeout(logTimeout, 5000);

        sandboxWorker.onmessage = (workerEvent) => {
          if (workerEvent.data.type === "READY") {
            sandboxWorker.postMessage({
              type: "INIT_WORKER",
              payload: {
                compiledModule,
                sharedMemory,
              },
            });
          }
          if (workerEvent.data.type === "INIT_COMPLETE") {
            resolve();
          }
          if (workerEvent.data.type === "INIT_FAILED") {
            console.log("[Engine JS] Sandbox init failed; retrying");
            sandboxWorker = new Worker(sandboxJs, { type: "module" });
          }
        };
      }).finally(() => {
        clearTimeout(timeoutId);
      });
    };

    await init({ module_or_path: compiledModule, memory: sharedMemory });
    console.log("[Engine JS] Main application WebAssembly module initialized.");

    let res = await engine_init();
    console.log(
      "[Engine JS] Main application WebAssembly module custom initialized: ",
      res
    );

    // start asset loader thread
    await new Promise((resolve, _reject) => {
      const assetLoader = new Worker(new URL("asset_loader.js", import.meta.url), { type: "module" });
      assetLoader.onmessage = (workerEvent) => {
        if (workerEvent.data.type === "READY") {
          assetLoader.postMessage({
            type: "INIT_ASSET_LOADER",
            payload: {
              compiledModule,
              sharedMemory,
            },
          });
        }
        if (workerEvent.data.type === "INITIALIZED") {
          resolve();
        }
      };
    });
  } catch (error) {
    console.error(
      "[Engine JS] Error during Wasm initialization or setup:",
      error
    );
    throw error;
  }
}

export async function startEngine(initialRealm, location, systemScene) {
  console.log(
    `[Main JS] Starting engine. Initial Realm: "${initialRealm}", Location: "${location}", System Scene: "${systemScene}"`
  );

  const platform = (() => {
    if (navigator.userAgent.includes("Mac")) return "macos";
    if (navigator.userAgent.includes("Win")) return "windows";
    if (navigator.userAgent.includes("Linux")) return "linux";
    return "unknown";
  })();

  engine_run(platform, initialRealm, location, systemScene, true, 1e6, 8);
}

// Import the wasm-bindgen generated JS glue code and Rust functions
import init, { engine_init, engine_run } from "./pkg/webgpu_build.js"; // Ensure this path is correct

const initialRealmInput = document.getElementById("initialRealm");
const locationInput = document.getElementById("location");
const systemSceneInput = document.getElementById("systemScene");
const initButton = document.getElementById("initButton");
const canvas = document.getElementById("mygame-canvas");

let initialRealmGroup = document.getElementById("initialRealm")?.parentElement;
let locationGroup = document.getElementById("location")?.parentElement;
let systemSceneGroup = document.getElementById("systemScene")?.parentElement;

function populateInputsFromQueryParams() {
  const queryParams = new URLSearchParams(window.location.search);
  const initialRealmParam = queryParams.get("initialRealm");
  if (initialRealmInput && initialRealmParam) {
    initialRealmInput.value = decodeURIComponent(initialRealmParam);
  } else if (initialRealmInput) {
    initialRealmInput.value = "https://realm-provider-ea.decentraland.org/main";
  }
  const locationParam = queryParams.get("location");
  if (locationInput && locationParam) {
    locationInput.value = decodeURIComponent(locationParam);
  } else if (locationInput) {
    locationInput.value = "0,0";
  }
  const systemSceneParam = queryParams.get("systemScene");
  if (systemSceneInput && systemSceneParam) {
    systemSceneInput.value = decodeURIComponent(systemSceneParam);
  } else if (systemSceneInput) {
    systemSceneInput.value = "";
  }
}
function hideSettings() {
  if (initialRealmGroup) initialRealmGroup.style.display = "none";
  if (locationGroup) locationGroup.style.display = "none";
  if (systemSceneGroup) systemSceneGroup.style.display = "none";
  if (initButton) initButton.style.display = "none";
}

var sharedMemory;

async function run() {
  populateInputsFromQueryParams();

  if (initButton) {
    initButton.disabled = true;
    initButton.textContent = "Loading...";
  }

  const wasmUrl = "./pkg/webgpu_build_bg.wasm";

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
      await fetch("./pkg/webgpu_build.js")
        .then((response) => response.text())
        .then((text) => new Blob([text], { type: "application/javascript" }))
    );

    const sandboxJs = URL.createObjectURL(
      await fetch("sandbox_worker.js")
        .then((response) => response.text())
        .then((text) => {
          const replacedText = text.replace("./pkg/webgpu_build.js", wasmJs);
          return new Blob([replacedText], { type: "application/javascript" });
        })
    );
    */

    window.spawn_and_init_sandbox = async () => {
      var timeoutId;
      return new Promise((resolve, _reject) => {
        // var sandboxWorker = new Worker(sandboxJs, { type: "module" });
        var sandboxWorker = new Worker("sandbox_worker.js", { type: "module" });

        var timeoutCount = 0;
        let logTimeout = () => {
          console.log(
            "[Main JS] Still waiting for worker to init",
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
            console.log("[Main JS] Sandbox init failed; retrying");
            sandboxWorker = new Worker(sandboxJs, { type: "module" });
          }
        };
      }).finally(() => {
        clearTimeout(timeoutId);
      });
    };

    await init({ module_or_path: compiledModule, memory: sharedMemory });
    console.log("[Main JS] Main application WebAssembly module initialized.");

    let res = await engine_init();
    console.log(
      "[Main JS] Main application WebAssembly module custom initialized: ", res
    );

    // start asset loader thread
    await new Promise((resolve, _reject) => {
      const assetLoader = new Worker("asset_loader.js", { type: "module" });
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

    if (initButton) {
      initButton.disabled = false;
      initButton.textContent = "Go";
    }

    initButton.onclick = () => {
      const initialRealm = initialRealmInput.value;
      const location = locationInput.value;
      const systemScene = systemSceneInput.value;
      console.log(
        `[Main JS] "Go" button clicked. Initial Realm: "${initialRealm}", Location: "${location}", System Scene: "${systemScene}"`
      );
      hideSettings();

      const platform = (() => {
        if (navigator.userAgent.includes("Mac")) return "macos";
        if (navigator.userAgent.includes("Win")) return "windows";
        if (navigator.userAgent.includes("Linux")) return "linux";
        return "unknown";
      })();

      engine_run(platform, initialRealm, location, systemScene, true, 1e6);
    };
  } catch (error) {
    console.error(
      "[Main JS] Error during Wasm initialization or setup:",
      error
    );
    if (initButton) {
      initButton.textContent = "Load Failed";
    }
  }
}

run().catch(console.error);

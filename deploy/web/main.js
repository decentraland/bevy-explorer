// Import the wasm-bindgen generated JS glue code and Rust functions
import { initGpuCache } from "./gpu_cache.js";
import init, { engine_init, engine_run, gpu_cache_hash } from "./pkg/webgpu_build.js"; // Ensure this path is correct

const initialRealmInput = document.getElementById("initialRealm");
const locationInput = document.getElementById("location");
const systemSceneInput = document.getElementById("systemScene");
const previewInput = document.getElementById("preview");
const initButton = document.getElementById("initButton");
const canvas = document.getElementById("canvas-parent");
const header = document.getElementById("header");

var autoStart = true;

const DEFAULT_SERVER = "https://realm-provider-ea.decentraland.org/main"
const DEFAULT_SYSTEMSCENE = "https://dclexplorer.github.io/bevy-ui-scene/BevyUiScene"

function populateInputsFromQueryParams() {
  const queryParams = new URLSearchParams(window.location.search);

  const manualParams = queryParams.get("manualParams");
  if (manualParams) {
    autoStart = false;
  }

  const initialRealmParam = queryParams.get("initialRealm");
  if (initialRealmInput && initialRealmParam) {
    initialRealmInput.value = decodeURIComponent(initialRealmParam);
  } else if (initialRealmInput) {
    initialRealmInput.value = DEFAULT_SERVER;
  }

  const locationParam = queryParams.get("location");
  if (locationInput && locationParam) {
    locationInput.value = decodeURIComponent(locationParam);
  } else if (locationInput) {
    locationInput.value = "";
  }

  const systemSceneParam = queryParams.get("systemScene");
  if (systemSceneInput && systemSceneParam) {
    systemSceneInput.value = decodeURIComponent(systemSceneParam);
  } else if (systemSceneInput) {
    systemSceneInput.value = DEFAULT_SYSTEMSCENE;
  }

  const previewParam = queryParams.get("preview");
  if (previewInput && previewParam) {
    previewInput.checked = true;
  } else if (previewInput) {
    previewInput.checked = false;
  }

  initialRealmInput.disabled = autoStart;
  locationInput.disabled = autoStart;
  systemSceneInput.disabled = autoStart;
  previewInput.disabled = autoStart;
}
function hideHeader() {
  if (header) header.style.display = "none";
  if (canvas) canvas.style.display = "block";
}

if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    const basePath = window.location.pathname.replace(/\/$/, ''); // removes trailing slash if present
    const serviceWorkerPath = new URL(`${basePath}/service_worker.js`, window.location.origin);

    navigator.serviceWorker
      .register(serviceWorkerPath)
      .then((registration) => {
        console.log(
          "Page: Service Worker registered successfully with scope: ",
          registration.scope
        );
      })
      .catch((error) => {
        console.log("Page: Service Worker registration failed: ", error);
      });
  });

  // make sure the worker stays around after a hard reload
  // 1. Check if a service worker is active and controlling the page.
  if (navigator.serviceWorker && navigator.serviceWorker.controller) {
    // SUCCESS CASE:
    // If the recovery flag is present, it means we just successfully
    // recovered from a hard reload. We can now remove the flag.
    if (sessionStorage.getItem('sw_reloaded')) {
      console.log('Service Worker recovery successful. Cleaning up flag.');
      sessionStorage.removeItem('sw_reloaded');
    }
    // Everything is fine, let the app load.
  } else {
    // 2. RECOVERY CASE: No service worker is in control.
    // This could be a first visit or a hard reload.
    if (navigator.serviceWorker && navigator.serviceWorker.getRegistration) {
      navigator.serviceWorker.getRegistration().then(registration => {
        // We only try to recover if a service worker is already registered.
        if (registration) {
          // Prevent an infinite reload loop.
          if (sessionStorage.getItem('sw_reloaded')) {
            sessionStorage.removeItem('sw_reloaded');
            console.error('Service Worker failed to take control after reload.');
          } else {
            // Set the flag and perform a standard reload.
            console.log('Page is uncontrolled. Reloading to activate Service Worker...');
            sessionStorage.setItem('sw_reloaded', 'true');
            window.location.reload();
          }
        }
      });
    }
  }
}

async function initEngine() {
  populateInputsFromQueryParams();

  if (initButton) {
    initButton.disabled = true;
    if (autoStart) {
      initButton.textContent = "Autostarting .."
    } else {
      initButton.textContent = "Loading ..."
    }
  }

  const publicUrl = window.PUBLIC_URL || ".";
  const wasmUrl = `${publicUrl}/pkg/webgpu_build_bg.wasm`;

  try {
    const compiledModule = await WebAssembly.compileStreaming(fetch(wasmUrl));

    const initialMemoryPages = 1280; // setting initial memory high causes malloc failures
    const maximumMemoryPages = 65536;
    const sharedMemory = new WebAssembly.Memory({
      initial: initialMemoryPages,
      maximum: maximumMemoryPages,
      shared: true,
    });
    window.wasm_memory = sharedMemory;

    window.setVideoSource = (video, src) => {
      async function isHlsStream(url) {
        try {
          const response = await fetch(url, {
            method: "HEAD",
            mode: "cors",
          });

          if (!response.ok) {
            return false;
          }

          const contentType = response.headers.get("Content-Type");

          if (contentType) {
            return (
              contentType.includes("application/vnd.apple.mpegurl") ||
              contentType.includes("application/x-mpegURL")
            );
          }

          return false;
        } catch (error) {
          return false;
        }
      }

      if (video.canPlayType("application/vnd.apple.mpegurl")) {
        video.src = src;
      } else if (Hls.isSupported()) {
        // check if we need hls
        setTimeout(async () => {
          if (await isHlsStream(src)) {
            var hls = new Hls();
            hls.loadSource(src);
            hls.attachMedia(video);
          } else {
            video.src = src;
          }
        }, 0);
      }
    };

    window.spawn_and_init_sandbox = async () => {
      var timeoutId;
      return new Promise((resolve, _reject) => {
        const basePath = window.location.pathname.replace(/\/$/, ''); // removes trailing slash if present
        const sandboxWorkerPath = new URL(`${basePath}/sandbox_worker.js`, window.location.origin);
        var sandboxWorker = new Worker(sandboxWorkerPath, { type: "module" });

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
      "[Main JS] Main application WebAssembly module custom initialized: ",
      res
    );

    // start asset loader thread
    await new Promise((resolve, _reject) => {
      const basePath = window.location.pathname.replace(/\/$/, ''); // removes trailing slash if present
      const assetLoaderPath = new URL(`${basePath}/asset_loader.js`, window.location.origin);

      const assetLoader = new Worker(assetLoaderPath, { type: "module" });
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

    // start asset processor thread
    await new Promise((resolve, _reject) => {
      const basePath = window.location.pathname.replace(/\/$/, ''); // removes trailing slash if present
      const assetLoaderPath = new URL(`${basePath}/asset_processor.js`, window.location.origin);

      const assetProcessor = new Worker(assetLoaderPath, { type: "module" });
      assetProcessor.onmessage = (workerEvent) => {
        if (workerEvent.data.type === "READY") {
          assetProcessor.postMessage({
            type: "INIT_ASSET_PROCESSOR",
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
      "[Main JS] Error during Wasm initialization or setup:",
      error
    );
    if (initButton) {
      initButton.textContent = "Load Failed";
    }
  }
}

function start() {
  const initialRealm = initialRealmInput.value;
  const location = locationInput.value;
  const systemScene = systemSceneInput.value;
  const preview = previewInput.checked;
  console.log(
    `[Main JS] "Go" button clicked. Initial Realm: "${initialRealm}", Location: "${location}", System Scene: "${systemScene}"`
  );
  hideHeader();

  const platform = (() => {
    if (navigator.userAgent.includes("Mac")) return "macos";
    if (navigator.userAgent.includes("Win")) return "windows";
    if (navigator.userAgent.includes("Linux")) return "linux";
    return "unknown";
  })();

  engine_run(platform, initialRealm, location, systemScene, true, preview, 1e7);
}

initButton.onclick = start;

initEngine()
  .then(() => initGpuCache(gpu_cache_hash()))
  .then(() => {
    if (autoStart) {
      start()
    } else {
      initButton.disabled = false;
      initButton.textContent = "Go";
    }
  })
  .catch((e) => {
    console.log("error", e);
    initButton.textContent = "Load Failed";
  });

window.set_url_params = (x, y, server, system_scene, preview) => {
  try {
    const urlParams = new URLSearchParams(window.location.search);

    urlParams.set("location", `${x},${y}`);

    if (server != DEFAULT_SERVER) {
      urlParams.set("initialServer", realm);
    } else {
      urlParams.delete("initialServer");
    }

    if (system_scene != DEFAULT_SYSTEMSCENE) {
      urlParams.set("systemScene", system_scene);
    } else {
      urlParams.delete("systemScene");
    }

    if (preview) {
      urlParams.set("preview", true);
    } else {
      urlParams.delete("preview");
    }

    const newPath = window.location.pathname + '?' + urlParams.toString();
    history.replaceState(null, '', newPath);
  } catch (e) {
    console.log(`set url params failed: ${e}`);
  }
}


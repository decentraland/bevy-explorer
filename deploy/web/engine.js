// Engine logic - ES module
// Handles WASM/WebGPU initialization and game execution

import init, { engine_init, engine_run, gpu_cache_hash } from "./pkg/webgpu_build.js";
import { initGpuCache } from "./gpu_cache.js";

// Re-export for main.js
export { gpu_cache_hash, initGpuCache };

/**
 * Fetches a URL with download progress tracking.
 * @param {string} url - URL to fetch
 * @param {function} onProgress - Callback with percentage (0-100)
 * @param {number|null} expectedSize - Expected uncompressed size in bytes (fallback when Content-Length is missing due to content-encoding)
 * @returns {Promise<ArrayBuffer>}
 */
async function fetchWithProgress(url, onProgress, expectedSize) {
  const response = await fetch(url);
  const contentLength = response.headers.get('Content-Length');
  const total = contentLength ? parseInt(contentLength, 10) : expectedSize;

  if (!total || !response.body) {
    // Fallback if no size info available or no streaming support
    const buffer = await response.arrayBuffer();
    onProgress(100);
    return buffer;
  }

  const reader = response.body.getReader();
  const chunks = [];
  let received = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    chunks.push(value);
    received += value.length;
    onProgress(Math.min((received / total) * 100, 100));
  }

  // Combine chunks into single ArrayBuffer
  const buffer = new Uint8Array(received);
  let position = 0;
  for (const chunk of chunks) {
    buffer.set(chunk, position);
    position += chunk.length;
  }

  return buffer.buffer;
}

/**
 * Initializes the WASM engine, shared memory, and worker threads.
 * @returns {Promise<void>}
 */
export async function initEngine() {

  const publicUrl = window.PUBLIC_URL || ".";
  const wasmUrl = `${publicUrl}/pkg/webgpu_build_bg.wasm`;

  // Fetch manifest for expected WASM size (used when Content-Length is missing due to CDN compression)
  let expectedWasmSize = null;
  try {
    const manifest = await fetch(`${publicUrl}/pkg/manifest.json`).then(r => r.json());
    expectedWasmSize = manifest.wasmSize;
  } catch (e) {
    console.warn("Could not load manifest.json, progress may not be accurate", e);
  }

  // Step 1: Download WASM with progress
  setLoadingStepActive('download');
  const wasmBytes = await fetchWithProgress(wasmUrl, (percent) => {
    setLoadingStepProgress('download', percent);
  }, expectedWasmSize);
  setLoadingStepCompleted('download');

  // Step 2: Compile WASM
  setLoadingStepActive('compile');
  console.time("compileTime")
  const compiledModule = await WebAssembly.compile(wasmBytes);
  console.timeEnd("compileTime") // 70 ms
  setLoadingStepCompleted('compile');

  const initialMemoryPages = 1280; // setting initial memory high causes malloc failures
  const maximumMemoryPages = 65536;
  const sharedMemory = new WebAssembly.Memory({
    initial: initialMemoryPages,
    maximum: maximumMemoryPages,
    shared: true,
  });
  window.wasm_memory = sharedMemory;

  // Setup HLS video source callback
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

  // Setup sandbox worker spawn callback
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
          sandboxWorker = new Worker(sandboxWorkerPath, { type: "module" });
        }
      };
    }).finally(() => {
      clearTimeout(timeoutId);
    });
  };

  // Step 3: Initialize engine
  setLoadingStepActive('init');
  await init({ module_or_path: compiledModule, memory: sharedMemory });
  console.log("[Main JS] Main application WebAssembly module initialized.");

  let res = await engine_init();
  console.log(
    "[Main JS] Main application WebAssembly module custom initialized: ",
    res
  );
  setLoadingStepCompleted('init');

  // Step 4: Start workers
  setLoadingStepActive('workers');
  setLoadingStepProgress('workers', 0);

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
  setLoadingStepProgress('workers', 50);

  // start asset processor thread
  await new Promise((resolve, _reject) => {
    const basePath = window.location.pathname.replace(/\/$/, ''); // removes trailing slash if present
    const assetProcessorPath = new URL(`${basePath}/asset_processor.js`, window.location.origin);

    const assetProcessor = new Worker(assetProcessorPath, { type: "module" });
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
  setLoadingStepCompleted('workers');
}

/**
 * Starts the game engine with values from the UI inputs.
 */
export function start() {
  const realmValue = realmInput.value;
  const positionValue = positionInput.value;
  const systemScene = systemSceneInput.value;
  const preview = previewInput.checked;
  console.log(
    `[Main JS] "Launch" button clicked. Initial Realm: "${realmValue}", Position (coords): "${positionValue}", System Scene: "${systemScene}"`
  );
  hideHeader();

  const platform = (() => {
    if (navigator.userAgent.includes("Mac")) return "macos";
    if (navigator.userAgent.includes("Win")) return "windows";
    if (navigator.userAgent.includes("Linux")) return "linux";
    return "unknown";
  })();

  engine_run(platform, realmValue, positionValue, systemScene, true, preview, 1e7);
  setTimeout(showCanvas,200)
}

// Engine logic - ES module
// Handles WASM/WebGPU initialization and game execution

import init, { engine_init, engine_start, engine_spawn_worker, engine_console_command, engine_home_scene, engine_terms_accepted, gpu_cache_hash, report_pointer_lock, media_host_main, audio_host_main, livekit_host_main, engine_prepare_render } from "./pkg/webgpu_build.js";
import { createSandboxHost } from "./sandbox_host.js";

// Re-export for main.js
export { engine_home_scene, engine_terms_accepted, gpu_cache_hash };

// Spawns the render worker ahead of start(), so gpu_cache.js warms its device there (src/web.rs
// engine_render_setup) while the user is still on the login screen; resolves once it has.
// start() attaches the engine to that worker.
export function prepareRender() {
  return new Promise((resolve, reject) => {
    window.__gpuCacheReady = resolve;
    try {
      engine_prepare_render(document.getElementById("mygame-canvas"), glueUrl);
    } catch (e) {
      reject(e);
    }
  });
}

// The compiled engine module and its shared memory: every worker (asset loader/processor, scene
// sandboxes, and the engine + render workers started by start()) instantiates the same module on
// the same memory.
let compiledModule = null;
let sharedMemory = null;

/**
 * Logs the app-level messages of a worker (bevy's own relay handles the workers' console
 * mirroring and page calls).
 * @param {string} name - worker name for logging
 * @returns {(e: MessageEvent) => void}
 */
function workerMessageHandler(name) {
  return (e) => {
    const data = e.data;
    if (!data) return;
    if (data.__bevy_ready !== undefined) console.log(`[Main JS] ${name} worker ready`);
    else if (data.type) console.log(`[Main JS] ${name} worker: ${data.type}`);
  };
}

/**
 * Starts a headless scene server for a local preview beside the client: a second instance of
 * the engine in a hidden frame (headless.js). For the embedder that opened the preview, never
 * the engine. `options` is an engine_run options object naming the preview realm. Resolves once
 * the server's workers are started; rejects when the realm already has a server in this browser.
 * @param {object} options
 * @returns {Promise<object>}
 */
export function startServer(options) {
  const frame = document.createElement("iframe");
  frame.style.display = "none";
  frame.src = new URL("./headless.html", import.meta.url).href;
  const started = new Promise((resolve, reject) => {
    frame.addEventListener("load", () => {
      try {
        frame.contentWindow.startHeadless(compiledModule, options).then(resolve, reject);
      } catch (e) {
        reject(e);
      }
    });
  });
  document.body.appendChild(frame);
  // a server that did not start leaves no frame, and with it no claim on its realm (headless.js)
  started.catch(() => frame.remove());
  return started;
}
window.__bevyStartServer = startServer;

/** The url of this module's wasm-bindgen glue, for bevy's workers to import. */
const glueUrl = new URL("./pkg/webgpu_build.js", import.meta.url).href;

/**
 * Resolves once a worker bevy spawned has run its entry (it posts `__bevy_ready`), or rejects if
 * it failed to (`__bevy_ready` with an `error`).
 * @param {Worker} worker
 * @returns {Promise<void>}
 */
function workerReady(worker) {
  return new Promise((resolve, reject) => {
    const onReady = (e) => {
      if (e.data?.__bevy_ready === undefined) return;
      worker.removeEventListener("message", onReady);
      if (e.data.error !== undefined) reject(new Error(e.data.error));
      else resolve();
    };
    worker.addEventListener("message", onReady);
  });
}

/**
 * Records an uncaught worker error as context for the crash watchdog. A worker
 * thread that traps (panic / OOM) while holding a lock can deadlock the other
 * shared-memory threads with no exception on the main thread; the resulting
 * heartbeat stall is what actually surfaces the overlay, with this as the reason.
 * @param {string} name - worker name for logging
 * @returns {(e: ErrorEvent) => void}
 */
function workerCrashHandler(name) {
  return (e) => {
    if (window.reportEngineError) {
      window.reportEngineError(e.message || `${name} worker crashed`, `${name} worker`);
    } else {
      console.error(`[Main JS] ${name} worker crashed`, e);
    }
  };
}

/**
 * Fetches a URL with download progress tracking.
 * @param {string} url - URL to fetch
 * @param {function} onProgress - Callback with percentage (0-100)
 * @param {number|null} expectedSize - Expected decoded size in bytes. Preferred over
 *   Content-Length, which under CDN compression counts compressed bytes while the body
 *   reader yields decoded bytes.
 * @returns {Promise<ArrayBuffer>}
 */
async function fetchWithProgress(url, onProgress, expectedSize) {
  const response = await fetch(url);
  const contentLength = response.headers.get('Content-Length');
  const total = expectedSize || (contentLength ? parseInt(contentLength, 10) : null);

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
 * Downloads and compiles the engine WASM.
 *
 * Compiles with WebAssembly.compileStreaming on the live network response, so
 * compilation overlaps the download and — because the response keeps its URL
 * identity — the browser may persist the compiled module in its code cache and
 * skip the compile on repeat visits. The progress callback is fed from a clone
 * of the response; wrapping a byte-counting stream in a synthesized Response
 * instead would discard the URL identity and with it the code cache.
 *
 * @param {string} url - WASM URL
 * @param {number|null} expectedSize - Expected decoded size in bytes (from the manifest)
 * @param {function} onProgress - Callback with percentage (0-100)
 * @param {function} onDownloaded - Called once the last byte has arrived, before the compile tail
 * @returns {Promise<WebAssembly.Module>}
 */
async function compileWasmWithProgress(url, expectedSize, onProgress, onDownloaded) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`WASM fetch failed: ${response.status} ${response.statusText}`);
  }

  const contentLength = response.headers.get('Content-Length');
  const total = expectedSize || (contentLength ? parseInt(contentLength, 10) : null);

  if (typeof WebAssembly.compileStreaming === 'function' && response.body) {
    let cancelProgress = () => {};
    let progressTask = Promise.resolve();
    if (total) {
      const reader = response.clone().body.getReader();
      cancelProgress = () => reader.cancel().catch(() => {});
      progressTask = (async () => {
        let received = 0;
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          received += value.length;
          onProgress(Math.min((received / total) * 100, 100));
        }
      })().catch(() => {});
    }

    try {
      const module = await WebAssembly.compileStreaming(response);
      await progressTask;
      onProgress(100);
      onDownloaded();
      return module;
    } catch (e) {
      // typically a server not sending Content-Type: application/wasm — refetch
      // (usually straight from the HTTP cache) and compile from a buffer instead
      cancelProgress();
      console.warn('[Main JS] compileStreaming failed, falling back to buffered compile', e);
    }
  } else {
    try {
      response.body?.cancel();
    } catch (e) { /* body already consumed or locked */ }
  }

  const buffer = await fetchWithProgress(url, onProgress, total);
  onDownloaded();
  return WebAssembly.compile(buffer);
}

/**
 * Initializes the WASM engine, shared memory, and worker threads.
 * @returns {Promise<void>}
 */
export async function initEngine() {

  // Versioned CDN base in prod (set by the host before boot); fall back to this module's own
  // directory — NOT the page path, which is the React app's URL in same-document mode.
  const publicUrl = window.PUBLIC_URL || new URL(".", import.meta.url).href.replace(/\/$/, "");
  const wasmUrl = `${publicUrl}/pkg/webgpu_build_bg.wasm`;

  // Fetch manifest for expected WASM size (used when Content-Length is missing due to CDN compression)
  let expectedWasmSize = null;
  try {
    const manifest = await fetch(`${publicUrl}/pkg/manifest.json`).then(r => r.json());
    expectedWasmSize = manifest.wasmSize;
  } catch (e) {
    console.warn("Could not load manifest.json, progress may not be accurate", e);
  }

  // Steps 1+2: Download and compile WASM. Compilation streams alongside the download,
  // so the 'compile' step only covers the tail left after the last byte arrives.
  setLoadingStepActive('download');
  console.time("compileTime")
  compiledModule = await compileWasmWithProgress(
    wasmUrl,
    expectedWasmSize,
    (percent) => setLoadingStepProgress('download', percent),
    () => {
      setLoadingStepCompleted('download');
      setLoadingStepActive('compile');
    }
  );
  console.timeEnd("compileTime")
  setLoadingStepCompleted('compile');

  const initialMemoryPages = 1280; // setting initial memory high causes malloc failures
  const maximumMemoryPages = 65536;
  sharedMemory = new WebAssembly.Memory({
    initial: initialMemoryPages,
    maximum: maximumMemoryPages,
    shared: true,
  });
  window.wasm_memory = sharedMemory;

  // Browser-only actions the engine relays here (copypwasmta via src/web.rs, crates/restricted_actions):
  // the engine worker has no clipboard and no window.open. The click or key press that triggered
  // them gives the page the transient activation they need.
  window.__copyToClipboard = (text) => navigator.clipboard.writeText(text);
  window.__readClipboard = () => navigator.clipboard.readText();
  window.__openExternalUrl = (url) => {
    if (!window.open(url, "_blank")) console.warn("[Main JS] window.open blocked:", url);
  };

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

  // The scene sandbox workers (sandbox_host.js), behind the page functions the engine calls.
  const sandboxes = createSandboxHost({
    compiledModule,
    sharedMemory,
    // Set by host pages that want the super-user scene's BroadcastChannel names scoped
    // to this tab (react-web seeds it before booting — issue #1089). Left unset, channel
    // names stay bare — embedders like creator-hub's inspector share the bus with the
    // scene from a DIFFERENT document (parent of the engine iframe), which can't see
    // this window's session id.
    bridgeSession: window.__bridgeSession,
    onWorkerCrash: workerCrashHandler("sandbox"),
  });
  window.terminate_sandbox = sandboxes.terminate;
  window.spawn_and_init_sandbox = sandboxes.spawn;

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

  // The asset loader and processor threads: workers of ours, spawned on bevy's terms
  // (src/web.rs init_asset_load_thread / image_processor_main).
  const assetLoader = engine_spawn_worker(glueUrl, "init_asset_load_thread", "asset loader");
  assetLoader.onerror = workerCrashHandler("asset loader");
  await workerReady(assetLoader);
  setLoadingStepProgress('workers', 50);

  const assetProcessor = engine_spawn_worker(glueUrl, "image_processor_main", "asset processor");
  assetProcessor.onerror = workerCrashHandler("asset processor");
  await workerReady(assetProcessor);
  setLoadingStepCompleted('workers');
}

/**
 * Writes launch options (an engine_run options object, or the engine's echo of it) into url params:
 * absent (null), an unset flag (false) or empty = removed, so the url only carries what is set.
 * Shared with boot.js's url sync.
 * @param {URLSearchParams} urlParams
 * @param {Record<string, string | boolean | null | undefined>} options
 */
export function applyOptionsToUrlParams(urlParams, options) {
  for (const [name, value] of Object.entries(options)) {
    if (value == null || value === false || value === '') urlParams.delete(name);
    else urlParams.set(name, value === true ? 'true' : String(value));
  }
}

/**
 * Starts the game engine. `options` is the engine_run options object — one named key per
 * launch parameter (LaunchOptions in crates/system_api_types/src/launch_options.rs: realm,
 * position, systemScene, portables, preview, editor, imposterSource, the service overrides). It comes from
 * boot.js's __bevyLaunch, fed by the React host. Absent keys take the engine's defaults; an
 * unknown key makes engine_run throw.
 */
export function start(options = {}) {
  // Launch at most once per page: a second engine_run re-runs init_runtime, whose OnceCell is
  // already set, and panics ("can't init wasm queue"). One engine per page — ignore re-entry.
  if (window.__bevyStarted) {
    console.warn('[engine] start() ignored — the engine is already running');
    return;
  }
  window.__bevyStarted = true;

  console.log(`[Main JS] launching with ${JSON.stringify(options)}`);
  hideHeader();

  // Callback invoked by Rust once console command metadata is available.
  window._buildEngineApi = (json) => {
    try {
      const api = JSON.parse(json);
      window.engine = {};
      for (const cmd of api) {
        const jsName = cmd.cmd
          .replace(/^\//, '')
          .replace(/_([a-z])/g, (_, c) => c.toUpperCase());
        const paramNames = cmd.args.map(a => a.name);
        const body = [
          `var parts = [${JSON.stringify(cmd.cmd)}];`,
          `var defs = ${JSON.stringify(cmd.args)};`,
          `for (var i = 0; i < defs.length; i++) {`,
          `  var val = arguments[i];`,
          `  if (val === undefined) { if (!defs[i].optional) throw new Error(${JSON.stringify(jsName)} + ": missing arg '" + defs[i].name + "'"); break; }`,
          `  parts.push(defs[i].kind === 'json' ? JSON.stringify(val) : String(val));`,
          `}`,
          `return window.engine_console_command(parts.join(' ')).then(function(r) { try { return JSON.parse(r); } catch(e) { return r; } });`,
        ].join('\n');
        const fn = new Function(...paramNames, body);
        const sig = cmd.args.map(a => {
          const name = a.kind === 'json' ? `${a.name}: object` : a.name;
          return a.optional ? `[${name}]` : `<${name}>`;
        }).join(', ');
        fn._sig = `(${sig})`;
        fn._help = cmd.help || '';
        fn.toString = () => `${jsName}${fn._sig}${fn._help ? ' — ' + fn._help : ''}`;
        window.engine[jsName] = fn;
      }
      window.engine.help = (name) => {
        if (!name) {
          const lines = ['Available commands:'];
          for (const [k, v] of Object.entries(window.engine)) {
            if (typeof v === 'function' && v._help) lines.push(`  ${k} - ${v._help}`);
          }
          return lines.join('\n');
        }
        const fn = window.engine[name];
        if (!fn?._sig) return `Unknown command: ${name}`;
        return `${name}${fn._sig}\n${fn._help || ''}`;
      };
    } catch (e) {
      console.warn('Failed to build engine API:', e);
    }
    delete window._buildEngineApi;
  };

  // The engine runs off the page: winit's DOM side stays here on the canvas, the app world runs
  // on an engine worker, the render world on a render worker that owns the canvas as an
  // OffscreenCanvas, and the ComputeTaskPool's systems on compute workers (bevy::web_worker,
  // src/web.rs engine_start), as many as the `computeThreads` launch option says, else sized
  // from the core count there.
  const canvas = document.getElementById("mygame-canvas");
  // Everything the host handed us goes through as-is (the engine rejects unknown keys).
  const { engine: engineWorker, render: renderWorker, compute: computeWorkers } = engine_start(
    canvas,
    options,
    glueUrl
  );
  console.log(`[Main JS] compute workers: ${computeWorkers.length}`);
  renderWorker.onerror = workerCrashHandler("render");
  renderWorker.addEventListener("message", workerMessageHandler("render"));
  computeWorkers.forEach((worker, i) => {
    worker.onerror = workerCrashHandler(`compute ${i}`);
    worker.addEventListener("message", workerMessageHandler(`compute ${i}`));
  });
  engineWorker.onerror = workerCrashHandler("engine");
  engineWorker.addEventListener("message", workerMessageHandler("engine"));
  // The console RPC is the host's "engine is up" signal (engineRpc.ts ready()); the engine
  // worker is ready once engine_run has returned, having wired the command channel.
  workerReady(engineWorker).then(() => {
    window.engine_console_command = engine_console_command;
  });

  // The engine worker cannot see the document's pointer-lock state (Escape, a refused request):
  // report it through the shared wasm memory (src/web.rs report_pointer_lock).
  document.addEventListener("pointerlockchange", () => report_pointer_lock(document.pointerLockElement === canvas));
  document.addEventListener("pointerlockerror", () => report_pointer_lock(false));

  // Html media (scene video/audio) lives here on the page, the only thread with a DOM; the
  // engine drives it over channels and the page transfers decoded frames to the render worker
  // (crates/media/src/html).
  media_host_main(renderWorker);
  // Scene audio sources: the WebAudio graphs live here too (crates/av/src/audio_host_wasm.rs).
  audio_host_main();
  // Livekit rooms: the livekit-client objects (WebRTC) live here (crates/comms/src/livekit/web/host.rs).
  livekit_host_main();

  window.loadSceneUtils = () => {
    return new Promise((resolve, reject) => {
      const s = document.createElement('script');
      s.src = new URL('./sceneUtils.js', import.meta.url);
      s.onload = () => { console.log('sceneUtils loaded'); resolve(); };
      s.onerror = () => reject(new Error('failed to load sceneUtils.js'));
      document.head.appendChild(s);
    });
  };
  setTimeout(showCanvas, 200);

  document.getElementById("mygame-canvas").started = true;
}

// Engine logic - ES module
// Handles WASM/WebGPU initialization and game execution

import init, { engine_init, engine_run, engine_console_command, engine_home_scene, gpu_cache_hash } from "./pkg/webgpu_build.js";
import { initGpuCache } from "./gpu_cache.js";

// Re-export for main.js
export { engine_home_scene, gpu_cache_hash, initGpuCache };

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
  const compiledModule = await compileWasmWithProgress(
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

  // Live sandbox workers by scene id (BigInt), for kill escalation — see terminate_sandbox.
  // Entries are added on SCENE_READY (a worker reports which scene it popped from the shared
  // queue) and removed on SHUTDOWN_COMPLETE (the worker's dying ack, posted on every exit
  // path just before it closes itself).
  const sandboxWorkers = new Map();
  // Scenes killed before their worker reported in; the escalation arms on SCENE_READY.
  const pendingKills = new Set();
  // Scenes whose worker exited on its own (scene error / graceful end) before the engine's
  // kill request arrived. The eventual terminate_sandbox call consumes the entry and
  // resolves immediately, instead of parking in pendingKills waiting on a SCENE_READY that
  // will never come.
  const completedScenes = new Set();
  const KILL_GRACE_MS = 5000;

  // killFlags layout (Int32Array over a per-worker SharedArrayBuffer, shared with the worker
  // script — NOT with scene code, which can't see the worker's module scope):
  //   [0] KILL: engine has decided to terminate; the worker's op wrapper parks instead of
  //       entering the engine wasm
  //   [1] IN_RUST: the worker is inside a wasm call (op wrapper), or anywhere in its
  //       teardown — held across the whole teardown because the drain's timer gaps let the
  //       executor poll draining op futures, and those polls enter the engine wasm unflagged
  //   [2] park target for Atomics.wait — never written
  const KILL = 0, IN_RUST = 1;

  // Tier-2 forceful kill: a worker that ignored SHUTDOWN is stuck in a sync spin. A naive
  // Worker.terminate() could land mid-op while the worker holds a lock inside the shared
  // engine wasm memory (allocator, channel mutex) and corrupt the engine, so handshake
  // first: set KILL, then wait for IN_RUST to clear. Once KILL is visible the op wrapper
  // parks before entering rust, so IN_RUST == 0 means the worker can never re-enter — the
  // spin is pure JS and terminate is safe. The worker's few unwrapped wasm calls all sit in
  // its bounded init path (pre-scene-code), where a 20s-unresponsive scene cannot be.
  //
  // The dead thread's state in the shared wasm memory is deliberately leaked: a parked
  // async op future may still reference it, and its waker can fire after the terminate
  // (channel close, comms), so freeing the stack/TLS here would corrupt the engine.
  const forceTerminate = (sceneId) => {
    const entry = sandboxWorkers.get(sceneId);
    if (!entry) return;
    Atomics.store(entry.killFlags, KILL, 1);
    const startedAt = performance.now();
    const tryTerminate = () => {
      // acked in the meantime — the worker got out on its own
      if (!sandboxWorkers.get(sceneId)) return;
      if (Atomics.load(entry.killFlags, IN_RUST) === 1) {
        if (performance.now() - startedAt > 10000) {
          console.error(`[Main JS] scene ${sceneId} is blocked inside engine wasm; leaving worker running`);
          return;
        }
        setTimeout(tryTerminate, 100);
        return;
      }
      entry.worker.terminate();
      sandboxWorkers.delete(sceneId);
      console.warn(`[Main JS] scene ${sceneId} forcibly terminated; thread state leaked`);
    };
    // defer so a SHUTDOWN_COMPLETE already queued by the worker gets processed first
    setTimeout(tryTerminate, 100);
  };

  // Called from the engine when it drops a scene's handle (despawn, or the watchdog marking
  // it broken). The kill flag is already set in shared wasm memory, so a healthy scene needs
  // nothing from us: it exits at its next tick boundary and acks with SHUTDOWN_COMPLETE. No
  // ack within the grace period means the scene is wedged in an await — post SHUTDOWN so the
  // worker tears itself down from its event loop. A worker that ignores that too is stuck in
  // a sync spin: forcefully terminate it (forceTerminate above).
  window.terminate_sandbox = (sceneId) => {
    const entry = sandboxWorkers.get(sceneId);
    if (!entry) {
      if (completedScenes.delete(sceneId)) {
        console.debug(`[Main JS] kill requested for scene ${sceneId}; worker already exited`);
        return;
      }
      console.debug(`[Main JS] kill requested for scene ${sceneId} before its worker reported in; deferred to SCENE_READY`);
      pendingKills.add(sceneId);
      return;
    }
    console.debug(`[Main JS] kill requested for scene ${sceneId}; awaiting graceful exit`);
    entry.timer = setTimeout(() => {
      console.warn(`[Main JS] scene ${sceneId} still running after kill; posting SHUTDOWN`);
      entry.worker.postMessage({ type: "SHUTDOWN", shutdownToken: entry.shutdownToken });
      entry.timer = setTimeout(() => {
        console.error(`[Main JS] scene ${sceneId} did not respond to SHUTDOWN (sync spin?); force-terminating`);
        forceTerminate(sceneId);
      }, KILL_GRACE_MS);
    }, KILL_GRACE_MS);
  };

  // Setup sandbox worker spawn callback
  window.spawn_and_init_sandbox = async () => {
    var timeoutId;
    return new Promise((resolve, _reject) => {
      // The BUNDLE, not sandbox_worker.js. Scene code shares this worker's realm, and any
      // module in that realm can be re-imported by URL — which for "./pkg/webgpu_build.js"
      // hands back OUR initialised instance (wasm-bindgen's init returns the cached exports),
      // shared engine heap and all. Inlining makes the glue's exports ordinary module-scope
      // bindings of the bundle instead. A scene can still import the bundle's URL, but a
      // namespace object exposes only that module's *exports*, and this entry has none — so
      // it gets an empty object. Keep sandbox_worker.js export-free or that stops being true.
      // Built alongside the wasm (see react-web/README.md); lives in pkg/ so the glue's
      // relative paths still resolve. sandbox_worker.js is unchanged, just no longer the entry.
      const sandboxWorkerPath = new URL("./pkg/sandbox_worker.bundle.js", import.meta.url);

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

      // Payload goes out unprompted — a worker queues messages posted before it has a listener,
      // so nothing needs to ask for it. Scene code shares this worker's realm and reaches the
      // real worker global (bare `postMessage` is the platform's, not ours), so any request we
      // honoured would be forgeable, and sharedMemory is the engine heap. This side keeps
      // listening while scene code runs (for the SCENE_READY / SHUTDOWN_COMPLETE kill-tracking
      // messages), so every message the worker script posts carries killToken — a per-worker
      // secret held in the worker's module scope, where scene code can't see it — and anything
      // without it is dropped as a forgery.
      const spawn = () => {
        const sandboxWorker = new Worker(sandboxWorkerPath, { type: "module" });
        sandboxWorker.onerror = workerCrashHandler("sandbox");
        const killToken = crypto.randomUUID();
        // Separate secret authenticating the inbound SHUTDOWN (scene code can synthesize
        // message events inside the worker via dispatchEvent). Deliberately NOT killToken:
        // a scene listening for messages observes a genuine SHUTDOWN and learns this token,
        // and must not thereby gain the ability to forge the worker→engine ack.
        const shutdownToken = crypto.randomUUID();
        const killFlags = new Int32Array(new SharedArrayBuffer(16));
        sandboxWorker.postMessage({
          type: "INIT_WORKER",
          payload: {
            compiledModule,
            sharedMemory,
            killFlags: killFlags.buffer,
            // Set by host pages that want the super-user scene's BroadcastChannel names scoped
            // to this tab (react-web seeds it before booting — issue #1089). Left unset, channel
            // names stay bare — embedders like creator-hub's inspector share the bus with the
            // scene from a DIFFERENT document (parent of the engine iframe), which can't see
            // this window's session id.
            bridgeSession: window.__bridgeSession,
            killToken,
            shutdownToken,
          },
        });
        let warnedForgery = false;
        sandboxWorker.onmessage = (workerEvent) => {
          if (workerEvent.data.killToken !== killToken) {
            // scene code can reach the bare postMessage; drop (and note) anything the
            // worker script didn't token
            if (!warnedForgery) {
              warnedForgery = true;
              console.warn("[Main JS] dropped untokened message from a sandbox worker (scene code posting to the engine?)", workerEvent.data && workerEvent.data.type);
            }
            return;
          }
          if (workerEvent.data.type === "INIT_COMPLETE") {
            resolve();
          }
          if (workerEvent.data.type === "INIT_FAILED") {
            // The failed worker closes itself; this replaces it, wired the same way.
            sandboxWorker.onmessage = null;
            console.log("[Main JS] Sandbox init failed; retrying");
            spawn();
          }
          if (workerEvent.data.type === "SCENE_READY") {
            console.debug(`[Main JS] sandbox worker running scene ${workerEvent.data.sceneId}`);
            sandboxWorkers.set(workerEvent.data.sceneId, {
              worker: sandboxWorker,
              timer: undefined,
              killFlags,
              shutdownToken,
            });
            if (pendingKills.delete(workerEvent.data.sceneId)) {
              window.terminate_sandbox(workerEvent.data.sceneId);
            }
          }
          if (workerEvent.data.type === "SHUTDOWN_COMPLETE") {
            const sid = workerEvent.data.sceneId;
            const entry = sandboxWorkers.get(sid);
            if (entry && entry.worker === sandboxWorker) {
              console.debug(`[Main JS] scene ${sid} worker exited cleanly`);
              clearTimeout(entry.timer);
              sandboxWorkers.delete(sid);
            } else if (sid !== undefined && !pendingKills.delete(sid)) {
              // no entry: the worker exited before the engine asked (scene error / graceful
              // end). Remember it so the eventual kill request resolves immediately.
              // (sid is undefined when init_scene failed before the worker knew its scene —
              // nothing to correlate.)
              completedScenes.add(sid);
            }
          }
        };
      };
      spawn();
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
    const assetLoaderPath = new URL("./asset_loader.js", import.meta.url);

    const assetLoader = new Worker(assetLoaderPath, { type: "module" });
    assetLoader.onerror = workerCrashHandler("asset loader");
    // Unprompted, as for the sandbox above.
    assetLoader.postMessage({
      type: "INIT_ASSET_LOADER",
      payload: {
        compiledModule,
        sharedMemory,
      },
    });
    assetLoader.onmessage = (workerEvent) => {
      if (workerEvent.data.type === "INITIALIZED") {
        assetLoader.onmessage = null;
        resolve();
      }
    };
  });
  setLoadingStepProgress('workers', 50);

  // start asset processor thread
  await new Promise((resolve, _reject) => {
    const assetProcessorPath = new URL("./asset_processor.js", import.meta.url);

    const assetProcessor = new Worker(assetProcessorPath, { type: "module" });
    assetProcessor.onerror = workerCrashHandler("asset processor");
    assetProcessor.postMessage({
      type: "INIT_ASSET_PROCESSOR",
      payload: {
        compiledModule,
        sharedMemory,
      },
    });
    assetProcessor.onmessage = (workerEvent) => {
      if (workerEvent.data.type === "INITIALIZED") {
        assetProcessor.onmessage = null;
        resolve();
      }
    };
  });
  setLoadingStepCompleted('workers');
}

/**
 * Starts the game engine. Values come from the caller (boot.js's __bevyLaunch, fed by the React
 * host) — the old boot page's form inputs are gone.
 */
export function start({ realm, position, systemScene, portables, preview, editor, pulseServer, imposterSource } = {}) {
  // Launch at most once per page: a second engine_run re-runs init_runtime, whose OnceCell is
  // already set, and panics ("can't init wasm queue"). One engine per page — ignore re-entry.
  if (window.__bevyStarted) {
    console.warn('[engine] start() ignored — the engine is already running');
    return;
  }
  window.__bevyStarted = true;

  const realmValue = realm ?? '';
  const positionValue = position ?? '';
  const systemSceneValue = systemScene ?? '';
  const portablesValue = portables ?? 'basiccontroller.dcl.eth';
  const previewValue = preview === true;
  const editorValue = editor === true;
  // Pulse server as host:port (the WebTransport port on web); empty = the engine's default.
  const pulseServerValue = pulseServer ?? '';
  // Base url of the imposter store; empty = the engine's default.
  const imposterSourceValue = imposterSource ?? '';

  // Build params from URL, overriding with form field values
  const urlParams = new URLSearchParams(window.location.search);
  urlParams.set("realm", realmValue);
  if (positionValue) {
    urlParams.set("position", positionValue);
  }
  urlParams.set("systemScene", systemSceneValue);
  urlParams.set("portables", portablesValue);
  urlParams.delete("initialRealm");
  if (previewValue) {
    urlParams.set("preview", "true");
  } else {
    urlParams.delete("preview");
  }
  if (editorValue) {
    urlParams.set("editor", "true");
  } else {
    urlParams.delete("editor");
  }
  if (pulseServerValue) {
    urlParams.set("pulseServer", pulseServerValue);
  } else {
    urlParams.delete("pulseServer");
  }
  if (imposterSourceValue) {
    urlParams.set("imposterSource", imposterSourceValue);
  } else {
    urlParams.delete("imposterSource");
  }
  const params = urlParams.toString();
  console.log(
    `[Main JS] "Launch" button clicked. Initial Realm: "${realmValue}", Position (coords): "${positionValue}", System Scene: "${systemSceneValue}", Portables: "${portablesValue}"`
  );
  hideHeader();

  const platform = (() => {
    if (navigator.userAgent.includes("Mac")) return "macos";
    if (navigator.userAgent.includes("Win")) return "windows";
    if (navigator.userAgent.includes("Linux")) return "linux";
    return "unknown";
  })();

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

  engine_run(platform, realmValue, positionValue, systemSceneValue, portablesValue, true, previewValue, editorValue, 1e7, params, pulseServerValue, imposterSourceValue);
  window.engine_console_command = engine_console_command;
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

// sandbox_worker.js - Runs inside the final Web Worker, the most isolated environment.

// Import the wasm-bindgen generated JS glue code.
import init, * as wasm_bindgen_exports from "./pkg/webgpu_build.js";

// The capability the trusted super-user scene is given below. Captured before the scrub so the
// constructor survives while the global does not.
const RealBroadcastChannel = self.BroadcastChannel;

// self.WebSocket = {}

console.log("[Sandbox Worker] Starting");

const allowListES2020 = [
  "Array",
  "ArrayBuffer",
  "BigInt",
  "BigInt64Array",
  "BigUint64Array",
  "Boolean",
  "DataView",
  "Date",
  "decodeURI",
  "decodeURIComponent",
  "encodeURI",
  "encodeURIComponent",
  "Error",
  "escape",
  "eval",
  "EvalError",
  "fetch",
  "Float32Array",
  "Float64Array",
  "Function",
  "Infinity",
  "Int16Array",
  "Int32Array",
  "Int8Array",
  "isFinite",
  "isNaN",
  "JSON",
  "Map",
  "Math",
  "NaN",
  "Number",
  "Object",
  "parseFloat",
  "parseInt",
  "Promise",
  "Proxy",
  "RangeError",
  "ReferenceError",
  "Reflect",
  "RegExp",
  "Set",
  "SharedArrayBuffer",
  "String",
  "Symbol",
  "SyntaxError",
  "TypeError",
  "Uint16Array",
  "Uint32Array",
  "Uint8Array",
  "Uint8ClampedArray",
  "undefined",
  "unescape",
  "URIError",
  "WeakMap",
  "WebSocket",
  "WeakSet",
];

// Remove an inherited property. Interface objects (BroadcastChannel, Worker) are own properties of
// the global and go with a plain delete; attributes like navigator.storage live on a prototype, and
// deleting them off the instance silently succeeds without removing anything.
function deleteFromPrototypeChain(obj, name) {
  for (let o = obj; o != null; o = Object.getPrototypeOf(o)) {
    if (Object.prototype.hasOwnProperty.call(o, name)) return delete o[name];
  }
  return false;
}

const jsContext = Object.create(null);
var jsProxy = undefined;
var jsPreamble = undefined;
// Per-boot bridge session id from engine.js (INIT_WORKER payload); see the isSuper block below.
var bridgeSession = undefined;
function createJsContext(wasmApi, context) {
  const isSuper = wasmApi.is_super(context);

  // The allowlist below cannot withhold a capability: jsProxy is only consulted for
  // `globalThis.X` property lookups, so a bare identifier in scene code resolves straight
  // through to the real worker global. Withholding therefore has to be a deletion from that
  // global. `Worker` goes with it — a nested worker is a fresh realm whose global has
  // BroadcastChannel back, which would undo the deletion in one line. (SharedWorker isn't
  // exposed to dedicated workers in Chromium, but delete it too for other engines.)
  //
  // Runs before preloadModules and before any scene code, so nothing untrusted has observed
  // the pre-scrub global. Deleting for super-user scenes as well keeps one code path: the
  // trusted scene gets the captured constructor back through jsContext, below.
  delete self.BroadcastChannel;
  delete self.Worker;
  delete self.SharedWorker;

  // OPFS is the origin's storage: config.json, the ipfs cache and every scene's localStorage all
  // hang off the one root that navigator.storage.getDirectory() hands out. The scene's own storage
  // no longer goes through it from here — crates/dcl_wasm/src/inner/local_storage.rs took a handle
  // to just the local_storage/ subtree during wasm_init_scene, above, and that handle stays live
  // once the accessor is gone.
  //
  // `delete navigator.storage` would return true and do nothing: WorkerNavigator exposes it on its
  // prototype, so the own-property delete succeeds against a property that was never there. Walk to
  // the prototype that actually holds it. (Same reason `delete self.navigator` is a no-op.)
  //
  // storageBuckets is the second door to the same API — navigator.storageBuckets.open(name) hands
  // back a bucket with its own getDirectory(). A named bucket can't reach the default bucket's
  // contents, so it isn't a route to engine state, but it is unmetered scene-controlled storage and
  // two scenes agreeing on a bucket name would have a shared filesystem.
  deleteFromPrototypeChain(self.navigator, "storage");
  deleteFromPrototypeChain(self.navigator, "storageBuckets");

  // IndexedDB is same-origin too, and holds more than its own data: platform/src/web_save.js keeps
  // the FileSystemDirectoryHandle for the user's picked scene folder there (db `dcl-editor`, store
  // `handles`), with readwrite permission already granted. A handle read back out of IndexedDB is
  // as live as the one that was stored, so a scene reaching it would get the user's real
  // filesystem, not origin-private storage. Nothing in this worker uses IndexedDB — web_save.js and
  // gpu_cache.js both run on the main thread.
  deleteFromPrototypeChain(self, "indexedDB");

  // CacheStorage is the last same-origin store the sandbox could see — it holds the ipfs fetch
  // cache (`ipfs-path-cache-v1`), so a scene could read every asset the client has pulled and, more
  // to the point, write to keys the loader later serves. Its users are elsewhere:
  // image_processing/src/processor/wasm_fs.rs runs under asset_processor.js, which engine.js spawns
  // as its own worker, and service_worker.js is a different context entirely.
  deleteFromPrototypeChain(self, "caches");

  // BroadcastChannel is a same-origin, serverless side channel — handed ONLY to the trusted
  // super-user (--system-scene) scene, so an embedded host page can drive it; ordinary scenes never see it
  // (it would otherwise let an untrusted scene coordinate with the page / other scenes off-network).
  //
  // Same-origin means same-origin across ALL tabs, so when the host page provided a session id
  // (react-web seeds window.__bridgeSession; engine.js forwards it), channel names are suffixed
  // with it — otherwise one tab's HUD drives every tab's bridge scene (issue #1089). Wrapping the
  // constructor here keeps the scene code unchanged; the page appends the same suffix
  // (bridgeChannelName()). No session id → bare names, for embedders whose bus partner is a
  // different document that can't see the engine window's id (creator-hub's inspector iframe).
  if (isSuper) {
    Object.defineProperty(jsContext, "BroadcastChannel", {
      configurable: false,
      value: bridgeSession
        ? class BroadcastChannel extends RealBroadcastChannel {
            constructor(name) {
              super(`${name}#${bridgeSession}`);
            }
          }
        : RealBroadcastChannel,
    });
  }

  const sceneLabel = context.get_scene_title();
  const sceneStartTime = performance.now();
  function scenePrefix() {
    const elapsed = (performance.now() - sceneStartTime) / 1000;
    return `[${sceneLabel} ${elapsed.toFixed(2)}]`;
  }

  const ops = Object.create(null);
  for (const exportName in wasmApi) {
    if (exportName.substring(0, 3) === "op_") {
      Object.defineProperty(ops, exportName, {
        configurable: false,
        get() {
          return (...args) => {
            // Tier-2 handshake (see forceTerminate in engine.js): flag the wasm entry, and
            // once the engine has decided to terminate this worker, park instead of entering
            // — never throw, scene code could catch. IN_RUST-first ordering pairs with the
            // engine's KILL-first + wait-for-IN_RUST==0, so a terminate can never land while
            // this thread holds a lock inside the shared engine wasm.
            Atomics.store(killFlags, IN_RUST, 1);
            if (Atomics.load(killFlags, KILL) === 1) {
              Atomics.store(killFlags, IN_RUST, 0);
              // last words: nothing after the wait ever runs
              console.warn(`[Sandbox Worker] scene ${sceneId}: kill flag set; parking until terminate`);
              Atomics.wait(killFlags, PARK, 0);
            }
            let result;
            try {
              // wrap ops to inject context arg
              result = wasmApi[exportName](context, ...args);
            } finally {
              Atomics.store(killFlags, IN_RUST, 0);
            }
            if (result && typeof result.then === "function") {
              // async op: track it while its future is live, so the teardown drain can
              // name what is still holding scene state if it fails to drain.
              // .then(dec, dec) not .finally: the derived promise must never reject
              // (the caller handles the original; a rejecting clone would surface as
              // an unhandled rejection)
              outstandingOps.set(exportName, (outstandingOps.get(exportName) || 0) + 1);
              const dec = () => {
                const n = outstandingOps.get(exportName) || 0;
                if (n > 1) outstandingOps.set(exportName, n - 1);
                else outstandingOps.delete(exportName);
              };
              result.then(dec, dec);
            }
            return result;
          };
        },
      });
    }
  }
  function formatLog(...values) {
    return values.map(v => {
      if (v === null) return 'null';
      if (v === undefined) return 'undefined';
      if (typeof v === 'object') { try { return JSON.stringify(v); } catch(e) { return String(v); } }
      return String(v);
    }).join(' ');
  }

  // Save references to the real browser console before overriding
  const browserConsole = {
    log: console.log.bind(console),
    warn: console.warn.bind(console),
    error: console.error.bind(console),
    debug: console.debug.bind(console),
    trace: console.trace.bind(console),
  };

  Object.defineProperty(jsContext, "console", {
    value: {
      log: (...args) => { browserConsole.log(scenePrefix(), ...args); ops.op_log("LOG " + formatLog(...args)); },
      info: (...args) => { browserConsole.log(scenePrefix(), ...args); ops.op_log("LOG " + formatLog(...args)); },
      debug: (...args) => { browserConsole.debug(scenePrefix(), ...args); ops.op_log("LOG " + formatLog(...args)); },
      trace: (...args) => { browserConsole.trace(scenePrefix(), ...args); ops.op_log("TRACE " + formatLog(...args)); },
      warning: (...args) => { browserConsole.error(scenePrefix(), ...args); ops.op_error("ERROR " + formatLog(...args)); },
      error: (...args) => { browserConsole.error(scenePrefix(), ...args); ops.op_error("ERROR " + formatLog(...args)); },
      warn: (...args) => { browserConsole.warn(scenePrefix(), ...args); ops.op_log("WARN " + formatLog(...args)); },
    },
  });

  const core = Object.create(null);
  Object.defineProperty(core, "ops", {
    configurable: false,
    value: ops,
  });
  const Deno = Object.create(null);
  Object.defineProperty(Deno, "core", {
    configurable: false,
    value: core,
  });
  Object.defineProperty(jsContext, "Deno", {
    configurable: false,
    value: Deno,
  });

  Object.defineProperty(jsContext, "require", {
    configurable: false,
    value: require,
  });
  Object.defineProperty(jsContext, "localStorage", {
    configurable: false,
    value: createWebStorageProxy(ops),
  });

  // if (!isSuper) {
  //   Object.defineProperty(jsContext, "fetch", {
  //     configurable: false,
  //     get() {
  //       return (url, options) => {    
  //         console.error('[Sandbox worker] Fetch request to', url, 'was intentionally blocked by the proxy.');
  //         return Promise.reject(new Error('This request has been intentionally failed by the proxy.'));
  //       };    
  //     }
  //   })
  //   Object.defineProperty(jsContext, "WebSocket", {
  //     configurable: false,
  //     get() {
  //       console.log("get WebSocket!!!")
  //       return {}
  //     }
  //   })
  // }

  jsProxy = new Proxy(jsContext, {
    has() {
      return true;
    },
    get(_target, propKey, _receiver) {
      if (propKey === "eval") return eval;
      if (propKey === "globalThis") return jsProxy;
      if (propKey === "global") return jsProxy;
      if (propKey === "undefined") return undefined;
      if (jsContext[propKey] !== undefined) return jsContext[propKey];
      if (allowListES2020.includes(propKey)) {
        return globalThis[propKey];
      }
      return undefined;
    },
  });

  const contextKeys = Object.getOwnPropertyNames(jsContext);
  const allGlobals = [...new Set([...allowListES2020, ...contextKeys])];
  jsPreamble = allGlobals
    .map((key) => `const ${key} = globalThis.${key};`)
    .join("\n");
}

const defer = Promise.resolve().then.bind(Promise.resolve());

async function runWithScope(code) {
  const module = { exports: {} };

  const func = new Function(
    "globalThis",
    "module",
    "exports",
    `${jsPreamble}\n\n;(function (globalThis, module, exports) {\n${code}\n}).call(globalThis, globalThis, module, exports);`
  );

  await defer(() => func.call(jsProxy, jsProxy, module, module.exports));
  return module.exports;
}

// prefetch all the requireable scripts before we replace the fetch function
var allowedModules = undefined;

async function preloadModules(context, fetch_fn) {
  const modules = [
    "~system/BevyExplorerApi",
    "~system/CommunicationsController",
    "~system/CommsApi",
    "~system/EngineApi",
    "~system/EnvironmentApi",
    "~system/EthereumController",
    "~system/Players",
    "~system/PortableExperiences",
    "~system/RestrictedActions",
    "~system/Runtime",
    "~system/Scene",
    "~system/SignedFetch",
    "~system/Testing",
    "~system/UserActionModule",
    "~system/UserIdentity",
    "~system/AdaptationLayerHelper",
  ];

  const promises = modules.map(async (key) => {
    try {
      const code = fetch_fn(context, key);
      const result = await runWithScope(code);
      return [key, result];
    } catch (e) {
      return undefined
    }
  });

  allowedModules = Object.fromEntries((await Promise.all(promises)).filter(e => e));
}

function require(moduleName) {
  let code = allowedModules[moduleName];
  if (!code) {
    throw "can't find module `" + moduleName + "`";
  }

  return code;
}

var wasm_init = undefined;
var wasmContext = undefined;
var sceneId = undefined;
// Per-worker secrets from engine.js (INIT_WORKER payload). Scene code shares this realm and
// can reach the bare postMessage and dispatchEvent, but module-scope vars are invisible to
// it (`new Function` scopes to the global only) — so killToken proves an outbound message
// came from this script, and shutdownToken proves an inbound SHUTDOWN came from engine.js.
// They are separate secrets: killToken never travels inbound after scene code runs, so a
// scene that listens for messages and observes a genuine SHUTDOWN (learning shutdownToken)
// still can't forge the SHUTDOWN_COMPLETE ack and pass itself off as exited.
var killToken = undefined;
var shutdownToken = undefined;
function postToEngine(message) {
  postMessage({ ...message, killToken });
}

// Tier-2 handshake flags, shared with engine.js (layout documented there; the dummy is
// replaced by the real SharedArrayBuffer from the INIT_WORKER payload). Module-scoped, so
// scene code can't reach them.
var killFlags = new Int32Array(new SharedArrayBuffer(16));
const KILL = 0, IN_RUST = 1, PARK = 2;

// Async ops whose futures are currently live (name -> count), maintained by the op
// wrapper. Purely diagnostic: names what still holds scene state when the teardown
// drain gives up (e.g. read-stream ops whose engine-side sender is never dropped).
const outstandingOps = new Map();

// Single teardown for every exit path: the graceful loop exit, the scene-error exit, and
// the engine's SHUTDOWN escalation. Posts the ack in all cases — it tells engine.js not to
// escalate further and to drop its worker map entry.
//
// The state can only be freed once nothing else references it: async ops hold a state
// reference (and the context wrapper's borrow) for as long as their future lives. The
// engine has already closed the channels and set the kill flag by the time this runs, so
// parked ops resume, complete inertly and drop their references — wait for that (bounded,
// under engine.js's escalation grace). A future awaiting something that never resolves
// keeps its reference forever; then freeing this thread's stack/TLS would corrupt the
// engine when the future's waker later fires, so leak the thread state instead.
const DRAIN_TIMEOUT_MS = 4000;
var toreDown = false;
function tearDown() {
  if (toreDown) return;
  toreDown = true;
  // IN_RUST is held for the ENTIRE teardown, not per wasm call: the drain's setTimeout gaps
  // exist so the executor can poll draining op futures, and those polls enter the engine
  // wasm without setting any flag — a terminate landing mid-poll would be exactly the
  // corruption forceTerminate exists to avoid. Never cleared: the worker closes itself at
  // the end, and the SHUTDOWN_COMPLETE ack (not the flag) tells the engine this worker is
  // done. Re-asserted each drain tick in case a scene op resumed in a gap and its
  // finally-clear clobbered it.
  Atomics.store(killFlags, IN_RUST, 1);
  if (!wasm_init || wasmContext === undefined) {
    finishTearDown(wasm_init !== undefined);
    return;
  }
  const startedAt = performance.now();
  const drain = () => {
    Atomics.store(killFlags, IN_RUST, 1);
    const refs = wasmContext.ref_count();
    if (refs > 1 && performance.now() - startedAt < DRAIN_TIMEOUT_MS) {
      setTimeout(drain, 100);
      return;
    }
    if (refs > 1) {
      // dropping under live references would panic (and freeing would corrupt the engine
      // when a parked future's waker later fires) — skip the drop and leak
      const parked = [...outstandingOps].map(([op, n]) => (n > 1 ? `${op} x${n}` : op)).join(", ");
      console.warn(`[Sandbox Worker] scene ${sceneId}: ${refs - 1} op future(s) still hold scene state after ${DRAIN_TIMEOUT_MS}ms (${parked || "untracked"})`);
      wasmContext = undefined;
      finishTearDown(false);
      return;
    }
    // refs == 1 still holds at the drop: nothing interleaves a sync block
    let stateFreed = false;
    try {
      wasm_bindgen_exports.drop_context(wasmContext);
      stateFreed = true;
    } catch (e) {
      console.error(`[Sandbox Worker] scene ${sceneId}: error dropping scene context:`, e);
    }
    wasmContext = undefined;
    finishTearDown(stateFreed);
  };
  drain();
}

function finishTearDown(destroyThread) {
  if (destroyThread) {
    wasm_init.__wbindgen_thread_destroy();
  } else if (wasm_init) {
    console.warn(`[Sandbox Worker] scene ${sceneId}: scene state still in use; leaking thread state`);
  }
  console.debug(`[Sandbox Worker] scene ${sceneId}: teardown complete`);
  postToEngine({ type: "SHUTDOWN_COMPLETE", sceneId });
  self.close();
}

var initialized = false;
self.onmessage = async (event) => {
  if (event.data && event.data.type === "SHUTDOWN") {
    // scene code can synthesize message events (dispatchEvent resolves through to the real
    // worker global); only engine.js knows shutdownToken, so anything without it is a forgery
    if (!shutdownToken || event.data.shutdownToken !== shutdownToken) {
      console.warn("[Sandbox Worker] dropped SHUTDOWN without valid token (scene code dispatching events?)");
      return;
    }
    // engine kill escalation: the kill flag is already set, but the scene never came back
    // to the loop check (wedged in an await). A genuine SHUTDOWN runs with an empty JS
    // stack, so no op is in flight; close() discards the parked scene task.
    console.warn("[Sandbox Worker] SHUTDOWN received, tearing down");
    tearDown();
    return;
  }
  if (event.data && event.data.type === "INIT_WORKER") {
    // one-shot: a synthetic re-INIT from scene code could swap killFlags/killToken out from
    // under the kill handshake, decoupling the flags the engine reads from the ones the op
    // wrapper sets — which would make a forceful terminate land mid-op. The genuine
    // INIT_WORKER always arrives before scene code exists, so latching closes this.
    if (initialized) {
      console.warn("[Sandbox Worker] dropped duplicate INIT_WORKER (scene code dispatching events?)");
      return;
    }
    initialized = true;
    const { compiledModule, sharedMemory } = event.data.payload;
    bridgeSession = event.data.payload.bridgeSession;
    killToken = event.data.payload.killToken;
    shutdownToken = event.data.payload.shutdownToken;
    killFlags = new Int32Array(event.data.payload.killFlags);

    if (!compiledModule || !sharedMemory) {
      console.error("[Sandbox Worker] Invalid payload received.");
      return;
    }

    try {
      // init wasm
      wasm_init = await init({
        module_or_path: compiledModule,
        memory: sharedMemory,
      });
    } catch (e) {
      console.error(
        "[Scene Worker] Error during Wasm instantiation or setup:",
        e
      );
      postToEngine({ type: `INIT_FAILED` });
      self.close();
      return;
    }

    postToEngine({ type: `INIT_COMPLETE` });

    // add listener to clean up on unhandled rejections
    self.addEventListener("unhandledrejection", (event) => {
      // Prevent the default browser action
      event.preventDefault();

      console.error(
        "[Sandbox worker] Unhandled Promise Rejection in Worker:",
        event.reason
      );

      // try {
      //   wasm_init.__wbindgen_thread_destroy();
      // } catch (cleanupError) {
      //   console.error(
      //     "[Sandbox worker] Error during WASM cleanup:",
      //     cleanupError
      //   );
      // }

      // self.close();
    });

    try {
      wasmContext = await wasm_bindgen_exports.wasm_init_scene();
    } catch (e) {
      console.error("[Scene Worker] Error during scene construction:", e);
      tearDown();
      return;
    }

    // report which scene this worker picked up (workers pop from a shared queue, so the
    // mapping isn't knowable at spawn time) — engine.js keeps a sceneId -> Worker map for
    // kill escalation
    sceneId = wasmContext.get_scene_id();
    postToEngine({ type: "SCENE_READY", sceneId });

    try {
      createJsContext(wasm_bindgen_exports, wasmContext);
      const ops = jsContext.Deno.core.ops;

      // preload modules
      await preloadModules(wasmContext, wasm_bindgen_exports.builtin_module);

      const sceneCode = wasmContext.get_source();
      let module = await runWithScope(sceneCode);

      // send any initial rpc requests
      ops.op_crdt_send_to_renderer([]);

      // the initial send drew on the tick's send allowance; give onStart a full one
      ops.op_set_elapsed(0);

      await module.onStart();

      var elapsed = 0;
      const startTime = new Date();
      var prevElapsed = 0;
      var elapsed = 0;
      var count = 0;
      var reportedErrors = 0;
      var consecutiveErrorsWithoutInteraction = 0;
      // Cap the per-frame dt handed to the scene: a slow frame must not feed dt-scaled scene
      // logic (timers, animations) a multi-second step. Mirrors MAX_SCENE_DT in
      // crates/dcl_deno/src/js/mod.rs.
      const MAX_SCENE_DT_SECONDS = 1;
      while (ops.op_continue_running()) {
        const dt = Math.min((elapsed - prevElapsed) / 1000, MAX_SCENE_DT_SECONDS);
        ops.op_set_elapsed(elapsed / 1000);
        try {
          await module.onUpdate(dt);
          consecutiveErrorsWithoutInteraction = 0;
        } catch (e) {
          reportedErrors += 1;
          consecutiveErrorsWithoutInteraction += 1;
          if (reportedErrors <= 10) {
            console.error(
              "[Sandbox worker] Error running onUpdate:",
              e
            );

            if (reportedErrors == 10) {
              console.error("[Sandbox worker] not logging any further uncaught errors.");
            }
          }

          if (ops.op_communicated_with_renderer()) {
            consecutiveErrorsWithoutInteraction = 0;
          }

          if (consecutiveErrorsWithoutInteraction >= 10) {
            throw "[Sandbox worker] too many errors without renderer interaction: shutting down";
          }
        }
        prevElapsed = elapsed;
        elapsed = new Date() - startTime;
        count += 1;
      }
      console.log("[Sandbox Worker] Exiting gracefully");
    } catch (e) {
      console.error("[Sandbox Worker] Error during scene execution:", e);
    }

    tearDown();
  }
};

function createWebStorageProxy(ops) {
  return new Proxy(
    {},
    {
      get(_target, prop, _receiver) {
        if (prop === "length") {
          return ops.op_webstorage_length();
        }

        if (prop === "getItem") {
          return (key) => ops.op_webstorage_get(String(key));
        }
        if (prop === "setItem") {
          return (key, value) =>
            ops.op_webstorage_set(String(key), String(value));
        }
        if (prop === "key") {
          return (index) => ops.op_webstorage_key(index);
        }
        if (prop === "removeItem") {
          return (key) => ops.op_webstorage_remove(String(key));
        }
        if (prop === "clear") {
          return () => ops.op_webstorage_clear();
        }

        // Handle direct property access like `localStorage.myKey`
        return ops.op_storage_get(String(prop));
      },

      set(_target, prop, value, _receiver) {
        ops.op_storage_set(String(prop), String(value));
        return true;
      },

      deleteProperty(_target, prop) {
        ops.op_webstorage_remove(String(prop));
        return true;
      },

      ownKeys(_target) {
        return ops.op_webstorage_iterate_keys();
      },

      getOwnPropertyDescriptor(_target, prop) {
        if (ops.op_webstorage_has(String(prop))) {
          return {
            value: ops.op_webstorage_get(String(prop)),
            writable: true,
            enumerable: true,
            configurable: true,
          };
        }
        return undefined;
      },

      has(_target, prop) {
        return ops.op_webstorage_has(String(prop));
      },
    }
  );
}


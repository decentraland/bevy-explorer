// sandbox_worker.js - Runs inside the final Web Worker, the most isolated environment.

// The scene runtime the scene runs on, in its own memory, and the copy module the engine half
// of the relay uses (relayStart below). The engine's own glue is never loaded here.
import initSceneRuntime, * as sceneRuntimeExports from "./pkg-scene/dcl_scene_wasm.js";
import { relayCopyWasm } from "./relay_copy.js";

// The capability the trusted super-user scene is given below. Captured before the scrub so the
// constructor survives while the global does not.
const RealBroadcastChannel = self.BroadcastChannel;
// Likewise this script's own way out: scene code can replace the globals, and what goes through
// postMessage carries killToken.
const workerPostMessage = self.postMessage.bind(self);
const workerClose = self.close.bind(self);
// Stack traces are only ever formatted the default way: scene code can't install a formatter.
Object.defineProperty(Error, "prepareStackTrace", {
  get: () => undefined,
  set: () => {},
  configurable: false,
});

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
  // the page's service worker registration (and its cache routes) is no scene's business
  deleteFromPrototypeChain(self.navigator, "serviceWorker");
  // the scene server's one-per-scene lock (headless.js) is held by name, which a scene could take
  deleteFromPrototypeChain(self.navigator, "locks");

  // IndexedDB is same-origin too, and holds more than its own data: platform/src/web_save.js keeps
  // the FileSystemDirectoryHandle for the user's picked scene folder there (db `dcl-editor`, store
  // `handles`), with readwrite permission already granted. A handle read back out of IndexedDB is
  // as live as the one that was stored, so a scene reaching it would get the user's real
  // filesystem, not origin-private storage. Nothing in this worker uses IndexedDB — web_save.js
  // runs on the main thread and gpu_cache.js on the render worker.
  deleteFromPrototypeChain(self, "indexedDB");

  // CacheStorage is the last same-origin store the sandbox could see — it holds the ipfs fetch
  // cache (`ipfs-path-cache-v2`), so a scene could read every asset the client has pulled and, more
  // to the point, write to keys the loader later serves. Its users are elsewhere:
  // image_processing/src/processor/wasm_fs.rs runs on the asset processor worker, which engine.js spawns
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
            // wrap ops to inject context arg
            const result = wasmApi[exportName](context, ...args);
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
      // JSON.stringify(new Error('x')) is '{}': an error's message and stack are not enumerable
      if (v instanceof Error) return v.stack ? `${v.name}: ${v.message}\n${v.stack}` : `${v.name}: ${v.message}`;
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

// the exports scene code runs against: the scene runtime's
const sceneApi = sceneRuntimeExports;
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
  workerPostMessage({ ...message, killToken });
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
// The relay goes first: it is all that holds engine-side state, and nothing after needs it.
// The scene state can only be freed once nothing else references it: async ops hold a state
// reference (and the context wrapper's borrow) for as long as their future lives. With the
// relay gone their channels are closed, so parked ops resume, complete inertly and drop their
// references — wait for that (bounded, under engine.js's escalation grace). A future awaiting
// something that never resolves keeps its reference forever; then the state is left for the
// worker's exit to discard. It is all in the scene runtime's own memory.
const DRAIN_TIMEOUT_MS = 4000;
var toreDown = false;
function tearDown() {
  if (toreDown) return;
  toreDown = true;
  relayFinish?.();
  if (!relayFinish || wasmContext === undefined) {
    finishTearDown(true);
    return;
  }
  const startedAt = performance.now();
  const drain = () => {
    const refs = wasmContext.ref_count();
    if (refs > 1 && performance.now() - startedAt < DRAIN_TIMEOUT_MS) {
      setTimeout(drain, 100);
      return;
    }
    if (refs > 1) {
      // dropping under live references would panic — skip the drop and leave it
      const parked = [...outstandingOps].map(([op, n]) => (n > 1 ? `${op} x${n}` : op)).join(", ");
      console.warn(`[Sandbox Worker] scene ${sceneId}: ${refs - 1} op future(s) still hold scene state after ${DRAIN_TIMEOUT_MS}ms (${parked || "untracked"})`);
      wasmContext = undefined;
      finishTearDown(false);
      return;
    }
    // refs == 1 still holds at the drop: nothing interleaves a sync block
    let stateFreed = false;
    try {
      sceneApi.drop_context(wasmContext);
      stateFreed = true;
    } catch (e) {
      console.error(`[Sandbox Worker] scene ${sceneId}: error dropping scene context:`, e);
    }
    wasmContext = undefined;
    finishTearDown(stateFreed);
  };
  drain();
}

function finishTearDown(stateFreed) {
  if (!stateFreed) {
    console.warn(`[Sandbox Worker] scene ${sceneId}: scene state still in use; leaking thread state`);
  }
  relayFinish?.();
  console.debug(`[Sandbox Worker] scene ${sceneId}: teardown complete`);
  postToEngine({ type: "SHUTDOWN_COMPLETE", sceneId });
  workerClose();
}

// The worker runs the scene on the scene runtime (B, its own memory, through its own glue), and
// an engine-module instance with NO glue (A) relays the scene's channels to and from the engine
// (crates/dcl_wasm/src/inner/relay.rs). A's relay imports are the copy module and B's relay
// exports (behind the guards in relayStart); every other import of A is a no-op stub.
// Nothing here keeps engine memory, A's exports or the copy module reachable from scene code:
// they live in this closure only.
//
// Captured before scene code runs, so scene code can't swap what the relay loop calls.
const relayAtomicsStore = Atomics.store;
const relayAtomicsLoad = Atomics.load;
const relayAtomicsWait = Atomics.wait;
const relaySetTimeout = setTimeout;
const relayPostTask = self.scheduler?.postTask?.bind(self.scheduler);
const relayLog = {
  debug: console.debug.bind(console),
  warn: console.warn.bind(console),
  error: console.error.bind(console),
};
var relayFinish = undefined;
async function relayStart(compiledModule, sharedMemory, sceneModule) {
  const scene = await initSceneRuntime({ module_or_path: sceneModule });
  const {
    instance: { exports: copy },
  } = await WebAssembly.instantiate(relayCopyWasm, { relay: { engine: sharedMemory, scene: scene.memory } });

  // IN_RUST is set only while this thread is inside A, which runs on engine memory: the engine
  // terminates the worker only once it is clear (engine.js forceTerminate). Set first, then
  // check KILL, pairing with the engine's KILL-first; once killed, park instead of entering.
  // Scene code shares this realm and may run inside a call to B (B's glue uses the globals), so
  // a call to B leaves A for its duration, and anything B throws is caught here: A never unwinds.
  const enterA = () => {
    relayAtomicsStore(killFlags, IN_RUST, 1);
    if (relayAtomicsLoad(killFlags, KILL) === 1) {
      relayAtomicsStore(killFlags, IN_RUST, 0);
      // last words: nothing after the wait ever runs
      relayLog.warn(`[Relay] scene ${sceneId}: kill flag set; parking until terminate`);
      relayAtomicsWait(killFlags, PARK, 0);
    }
  };
  const leaveA = () => relayAtomicsStore(killFlags, IN_RUST, 0);
  // Scene code running inside a call to B could reach something that enters A (teardown's
  // relayFinish): nothing may enter A while A is on the stack, and calls to B keep the depth.
  let depth = 0;
  const enter = (f) => {
    if (depth > 0) throw new Error("relay re-entered");
    depth += 1;
    enterA();
    try {
      return f();
    } finally {
      leaveA();
      depth -= 1;
    }
  };
  let failure = undefined;
  // fixed arity: spreading would run the array iterator, which scene code can replace
  const callB = (f) => (a, b, c) => {
    leaveA();
    let result = 0;
    try {
      result = f(a, b, c);
    } catch (e) {
      failure ??= e;
    }
    enterA();
    return result;
  };
  const relayImports = {
    relay_copy_to_b: copy.copy_to_b,
    relay_copy_from_b: copy.copy_from_b,
    relay_b_alloc: callB(scene.relay_b_alloc),
    relay_b_deliver: callB(scene.relay_b_deliver),
    relay_b_pump: callB(scene.relay_b_pump),
    relay_b_next_len: callB(scene.relay_b_next_len),
    relay_b_next_ptr: callB(scene.relay_b_next_ptr),
    relay_b_pop: callB(scene.relay_b_pop),
  };

  let relay;
  const imports = {};
  for (const { module, name, kind } of WebAssembly.Module.imports(compiledModule)) {
    imports[module] ??= {};
    // shimmed (`__wbg_<name>_<hash>`) or, for imports needing no conversion, the bare name
    const relayName = /^(?:__wbg_)?(relay_[a-z_]+?)(?:_[0-9a-f]{16})?$/.exec(name)?.[1];
    if (kind === "memory") {
      imports[module][name] = sharedMemory;
    } else if (relayName && relayImports[relayName]) {
      imports[module][name] = relayImports[relayName];
    } else if (name === "__wbindgen_init_externref_table") {
      // as the glue's: touches only this instance's externref table, never memory
      imports[module][name] = () => {
        const table = relay.__wbindgen_externrefs;
        const offset = table.grow(4);
        table.set(0, undefined);
        table.set(offset + 0, undefined);
        table.set(offset + 1, null);
        table.set(offset + 2, true);
        table.set(offset + 3, false);
      };
    } else {
      imports[module][name] = () => {};
    }
  }
  relay = (await WebAssembly.instantiate(compiledModule, imports)).exports;
  // what the glue's __wbg_finalize_init does: stack and TLS for this thread
  enter(() => relay.__wbindgen_start());
  postToEngine({ type: "INIT_COMPLETE" });

  // B failed under a relay call: end the scene
  const checkFailure = () => {
    if (failure === undefined) return false;
    relayLog.error(`[Relay] scene ${sceneId}: scene runtime failed; ending the scene`, failure);
    enter(() => relay.relay_fail());
    return true;
  };
  const logStats = () => {
    const stat = (i) => relay.relay_stat(i);
    relayLog.debug(
      `[Relay] scene ${sceneId}: to scene ${stat(0)}, to engine ${stat(1)}, bytes ${stat(2)}, failures ${stat(3)}, limits broken ${stat(4)}, system api ${stat(5)}`
    );
  };

  // the engine pushes the scene before spawning us, but another scene's push can hold the lock
  let got = 0;
  for (let attempt = 0; attempt < 100 && !got; attempt++) {
    got = enter(() => relay.relay_init());
    if (!got) await new Promise((resolve) => relaySetTimeout(resolve, 10));
  }
  if (!got) throw new Error("no scene in the relay queue");
  // from the engine's side, not the scene runtime's
  sceneId = BigInt.asUintN(64, enter(() => relay.relay_scene_id()));

  let done = false;
  let finishRequested = false;
  relayFinish = () => {
    if (done) return;
    if (depth > 0) {
      // A is on the stack: step finishes once it returns
      finishRequested = true;
      return;
    }
    done = true;
    try {
      enter(() => {
        logStats();
        relay.relay_drop();
        relay.__wbindgen_thread_destroy();
      });
    } catch (e) {
      relayLog.error(`[Relay] scene ${sceneId}: relay teardown failed; leaking its thread state`, e);
    }
  };

  // poll both, then sleep: yield to the event loop rather than block. The options object has no
  // prototype, so reading its members can't reach anything scene code defined.
  const sleep = (ms, f) =>
    relayPostTask ? relayPostTask(f, { __proto__: null, delay: ms }) : relaySetTimeout(f, ms);
  const step = () => {
    if (done) return;
    let killed = 0;
    let moved = 0;
    try {
      enter(() => {
        killed = relay.relay_killed();
        if (!killed) moved = relay.relay_pump();
      });
      if (checkFailure()) killed = 1;
    } catch (e) {
      relayLog.error(`[Relay] scene ${sceneId}: relay failed; ending the scene`, e);
      killed = 1;
    }
    if (finishRequested) {
      relayFinish();
      return;
    }
    if (killed) {
      // ends the scene loop; its teardown calls relayFinish
      try {
        sceneApi.relay_b_kill();
      } catch (e) {
        relayLog.error(`[Relay] scene ${sceneId}: scene runtime failed to stop`, e);
      }
      return;
    }
    sleep(moved ? 0 : 1, step);
  };
  step();
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
      await relayStart(compiledModule, sharedMemory, event.data.payload.sceneModule);
    } catch (e) {
      console.error("[Relay] start failed:", e);
      // after INIT_COMPLETE the engine won't respawn us, so ack the exit instead
      postToEngine({ type: relayFinish ? "SHUTDOWN_COMPLETE" : "INIT_FAILED", sceneId });
      workerClose();
      return;
    }

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
      wasmContext = await sceneApi.wasm_init_scene();
    } catch (e) {
      console.error("[Scene Worker] Error during scene construction:", e);
      tearDown();
      return;
    }

    // report which scene this worker picked up (workers pop from a shared queue, so the
    // mapping isn't knowable at spawn time; relayStart read it from the relay) — engine.js
    // keeps a sceneId -> Worker map for kill escalation
    postToEngine({ type: "SCENE_READY", sceneId });

    try {
      createJsContext(sceneApi, wasmContext);
      const ops = jsContext.Deno.core.ops;

      // preload modules
      await preloadModules(wasmContext, sceneApi.builtin_module);

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


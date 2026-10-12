// The scene sandbox workers of one engine instance: spawning them over that instance's module and
// memory, and the kill escalation. The page functions `spawn_and_init_sandbox` and
// `terminate_sandbox` (crates/dcl_wasm) are `spawn` and `terminate` of the host made here.

// the scene runtime every sandbox worker runs its scene on, compiled once for all of them; a
// failed fetch or compile isn't kept, so the next spawn tries again
let sceneRuntimeModule;
const compileSceneRuntime = () =>
  (sceneRuntimeModule ??= WebAssembly.compileStreaming(
    fetch(new URL("./pkg-scene/dcl_scene_wasm_bg.wasm", import.meta.url))
  ).catch((e) => {
    sceneRuntimeModule = undefined;
    throw e;
  }));

/**
 * @param {{ compiledModule: WebAssembly.Module, sharedMemory: WebAssembly.Memory,
 *           bridgeSession?: string, onWorkerCrash: (e: ErrorEvent) => void,
 *           role?: "server" }} engine
 * @returns {{ spawn: () => Promise<void>, terminate: (sceneId: bigint) => void }}
 */
export function createSandboxHost({ compiledModule, sharedMemory, bridgeSession, onWorkerCrash, role }) {
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
  //   [0] KILL: engine has decided to terminate; the worker's relay parks instead of
  //       entering the engine wasm
  //   [1] IN_RUST: the worker is inside the relay, the only code it runs on engine memory
  //   [2] park target for Atomics.wait — never written
  const KILL = 0, IN_RUST = 1;

  // Tier-2 forceful kill: a worker that ignored SHUTDOWN is stuck in a sync spin. A naive
  // Worker.terminate() could land mid-op while the worker holds a lock inside the shared
  // engine wasm memory (allocator, channel mutex) and corrupt the engine, so handshake
  // first: set KILL, then wait for IN_RUST to clear. Once KILL is visible the relay parks
  // before entering the engine wasm, so IN_RUST == 0 means the worker can never re-enter —
  // whatever it is spinning in, terminate is safe for the engine.
  //
  // The dead relay's state in the shared wasm memory is deliberately leaked: engine threads
  // can still wake its tasks after the terminate (channel close), so freeing its stack/TLS
  // here would corrupt the engine.
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
  const terminate = (sceneId) => {
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

  // The worker runs its scene on the scene runtime, in that runtime's own memory, and relays it
  // to the engine through an engine-module instance with no glue (crates/dcl_wasm/src/inner/relay.rs).
  const spawnAndInit = async () => {
    let sceneModule;
    // the engine waits on this for its queued scene, so keep trying rather than fail it
    while (!sceneModule) {
      try {
        sceneModule = await compileSceneRuntime();
      } catch (e) {
        console.error("[Main JS] scene runtime failed to load; retrying", e);
        await new Promise((resolve) => setTimeout(resolve, 1000));
      }
    }
    var timeoutId;
    return new Promise((resolve, _reject) => {
      // The BUNDLE, not sandbox_worker.js. Scene code shares this worker's realm, and any
      // module in that realm can be re-imported by URL. Inlining makes the scene runtime's glue
      // and the relay's modules ordinary module-scope bindings of the bundle instead. A scene
      // can still import the bundle's URL, but a namespace object exposes only that module's
      // *exports*, and this entry has none — so it gets an empty object. Keep sandbox_worker.js
      // export-free or that stops being true. Built alongside the wasm (see react-web/README.md).
      // A server's sandboxes are told apart by url: the service worker opens preview storage to them.
      const sandboxWorkerPath = new URL(`./pkg/sandbox_worker.bundle.js${role === "server" ? "?server" : ""}`, import.meta.url);

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
        sandboxWorker.onerror = onWorkerCrash;
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
            bridgeSession,
            killToken,
            shutdownToken,
            sceneModule,
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
              terminate(workerEvent.data.sceneId);
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

  return { spawn: spawnAndInit, terminate };
}

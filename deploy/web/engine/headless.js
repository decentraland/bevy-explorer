// A headless scene server beside the client: a second instance of the engine module over its own
// memory (src/headless/web.rs), so its statics, the server-mode flag among them, are its own.
// It runs in its own document (headless.html, a hidden frame of the client's page), which gives
// its page side (the glue instance, the page functions its workers call) its own global too.

import { createSandboxHost } from "./sandbox_host.js";

const RESTART_WAIT_MS = 5000;

/**
 * One server per preview realm across the browser's tabs, as the native server's scene lock
 * (src/bin/headless.rs claim_scene_locks): a second one would take the first's place in the
 * scene room. The lock is this document's, so it goes with the frame.
 * @param {string} realm
 * @returns {Promise<boolean>} false when another server holds the realm
 */
function claimRealm(realm) {
  return new Promise((resolve, reject) => {
    navigator.locks
      // a restart starts the next server as it removes the last one's frame: wait for its release
      .request(`dcl-scene-server:${realm}`, { signal: AbortSignal.timeout(RESTART_WAIT_MS) }, () => {
        resolve(true);
        return new Promise(() => {});
      })
      .catch((e) => (e?.name === "TimeoutError" || e?.name === "AbortError" ? resolve(false) : reject(e)));
  });
}

/**
 * Rejects when the realm already has a server in this browser.
 * @param {WebAssembly.Module} compiledModule - the engine module, as the client compiled it
 * @param {object} options - an engine_run options object; the launch options are read
 * @returns {Promise<{ engine: Worker, compute: Worker[], memory: WebAssembly.Memory }>}
 */
// the server's scene console output (src/headless/web.rs forward_scene_logs), newest last
const SCENE_LOG_LINES = 500;
const sceneLogs = [];
let sceneLogSeq = 0;

/**
 * @param {number} after - the last `seq` already read
 * @returns {Array<{ seq: number, level: string, msg: string }>}
 */
export function sceneLogsAfter(after) {
  return sceneLogs.filter((line) => line.seq > after);
}

function loadLivekit() {
  if (window.LivekitClient !== undefined) return Promise.resolve();
  const theirs = window.parent.document.querySelector('script[src*="livekit-client"]');
  if (theirs === null) return Promise.reject(new Error("the client page has no livekit-client"));
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = theirs.src;
    script.integrity = theirs.integrity;
    script.crossOrigin = "anonymous";
    script.onload = () => resolve();
    script.onerror = () => reject(new Error("livekit-client failed to load"));
    document.head.appendChild(script);
  });
}

export async function startHeadless(compiledModule, options) {
  if (!(await claimRealm(options.realm))) {
    throw new Error(`headless: ${options.realm} already has a scene server in this browser`);
  }
  const sharedMemory = new WebAssembly.Memory({ initial: 1280, maximum: 65536, shared: true });
  const glueUrl = new URL("./pkg/webgpu_build.js", import.meta.url).href;
  const glue = await import(glueUrl);
  await glue.default({ module_or_path: compiledModule, memory: sharedMemory });

  const sandboxes = createSandboxHost({
    compiledModule,
    sharedMemory,
    role: "server",
    onWorkerCrash: (e) => console.error("[headless] sandbox worker crashed", e),
  });
  window.terminate_sandbox = sandboxes.terminate;
  window.spawn_and_init_sandbox = sandboxes.spawn;

  // Livekit rooms: this frame's own copy of the client page's library (EngineHost.tsx loads it),
  // so the rooms and their connections belong to this document and close when the frame goes.
  await loadLivekit();
  glue.livekit_host_main();
  const { engine, compute } = glue.headless_start(options, glueUrl);
  engine.onerror = (e) => console.error("[headless] engine worker crashed", e);
  engine.addEventListener("message", (e) => {
    const log = e.data?.sceneLog;
    if (log === undefined) return;
    sceneLogs.push({ seq: ++sceneLogSeq, level: log.level, msg: log.msg });
    if (sceneLogs.length > SCENE_LOG_LINES) sceneLogs.shift();
  });
  return { engine, compute, memory: sharedMemory };
}

// A headless scene server beside the client: a second instance of the engine module over its own
// memory (src/headless/web.rs), so its statics, the server-mode flag among them, are its own.
// It runs in its own document (headless.html, a hidden frame of the client's page), which gives
// its page side (the glue instance, the page functions its workers call) its own global too.

import { createSandboxHost } from "./sandbox_host.js";

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
      .request(`dcl-scene-server:${realm}`, { ifAvailable: true }, (lock) => {
        resolve(lock !== null);
        return lock && new Promise(() => {});
      })
      .catch(reject);
  });
}

/**
 * Rejects when the realm already has a server in this browser.
 * @param {WebAssembly.Module} compiledModule - the engine module, as the client compiled it
 * @param {object} options - an engine_run options object; the launch options are read
 * @returns {Promise<{ engine: Worker, compute: Worker[], memory: WebAssembly.Memory }>}
 */
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
    onWorkerCrash: (e) => console.error("[headless] sandbox worker crashed", e),
  });
  window.terminate_sandbox = sandboxes.terminate;
  window.spawn_and_init_sandbox = sandboxes.spawn;

  // Livekit rooms: the livekit-client objects (WebRTC) live on this document. The library is
  // the client page's (EngineHost.tsx loads it), which a same-origin frame can reach.
  if (window.parent !== window) window.LivekitClient ??= window.parent.LivekitClient;
  glue.livekit_host_main();
  const { engine, compute } = glue.headless_start(options, glueUrl);
  engine.onerror = (e) => console.error("[headless] engine worker crashed", e);
  return { engine, compute, memory: sharedMemory };
}

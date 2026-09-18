import { createWorkerRpc } from './worker-rpc.js';
import { installLivekitWorker } from './worker-livekit.js';

installLivekitWorker(self, globalThis);

// wasm-bindgen's window-qualified application imports target these forwarders.
// This alias is not a DOM Window: web_sys::window() still returns None.
globalThis.window = globalThis;
let wasm;
let started = false;
const rpc = createWorkerRpc(self, {
  async init(module, memory, cache) {
    if (wasm) throw new Error('Engine worker already initialized');
    const store = new Map(Object.entries(cache));
    globalThis.localStorage = {
      getItem: key => store.get(key) ?? null,
      setItem(key, value) {
        store.set(key, String(value));
        rpc.notify('persistCache', key, String(value));
      },
    };
    wasm = await import('./pkg/webgpu_build.js');
    await wasm.default({ module_or_path: module, memory });
  },
  async gpuCache(key) {
    const { initGpuCache } = await import('./gpu_cache.js');
    await initGpuCache(key);
  },
  start(id, canvas, options) {
    if (started) throw new Error('Engine worker already started');
    wasm.engine_attach_worker(id, canvas);
    started = true;
    wasm.engine_run(options);
  },
  console(command) { return wasm.engine_console_command(command); },
  browserState(pointerLocked, fullscreen, available) {
    wasm.engine_browser_state(pointerLocked, fullscreen, available);
  },
});

for (const name of ['_buildEngineApi', 'set_url_params', '__setEngineTextFocus',
  'setLoadingStepActive', 'setLoadingStepCompleted', 'setLoadingStepProgress', 'terminate_sandbox']) {
  globalThis[name] = (...args) => rpc.notify(name, ...args);
}
globalThis.spawn_and_init_sandbox = () => rpc.call('spawn_and_init_sandbox');
globalThis.__setShaderCompiling = visible => rpc.notify('shaderCompiling', visible);
globalThis.__engineHeartbeat = () => rpc.notify('__engineHeartbeat');
self.addEventListener('error', event => rpc.notify('error', event.message));
self.addEventListener('unhandledrejection', event => rpc.notify('error', String(event.reason)));

globalThis.__dclAudioPcm = (id, samples, sampleRate) => {
  const copy = Float32Array.from(samples);
  self.postMessage({ type: 'engine-rpc', method: 'audioPcm', args: [id, copy, sampleRate] }, [copy.buffer]);
};
globalThis.__dclAudioOp = json => rpc.notify('audioOp', json);
globalThis.__dclAudioParams = data => {
  const copy = Float64Array.from(data);
  self.postMessage({ type: 'engine-rpc', method: 'audioParams', args: [copy] }, [copy.buffer]);
};

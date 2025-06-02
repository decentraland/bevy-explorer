// sandbox_worker.js - Runs inside the final Web Worker, the most isolated environment.

// Import the wasm-bindgen generated JS glue code.
import init, * as wasm_bindgen_exports from './pkg/webgpu_build.js';

console.log("[Sandbox Worker] Script loaded. Awaiting 'INIT_WORKER' message.");

const selfWorker = self;

function addDenoOps(wasmApi, context) {
    const ops = {};

    for (const exportName in wasmApi) {
        console.log("checking", exportName);
        if (exportName.substring(0,3) === "op_") {
            console.log("adding", exportName);
            ops[exportName] = (...args) => {
                // wrap ops to inject context arg
                return wasmApi[exportName](context, ...args);
            };
        }
    }

    globalThis.Deno = { core: { ops } };
}

function secureGlobalScope() {
    // This is our blacklist for the worker's own global scope.
    // const sensitiveApis = [
    //     // 'Worker', 'SharedWorker', 'importScripts', 'navigator',
    //     // 'fetch', 'XMLHttpRequest', 'WebSocket', 'EventSource', 'RTCDataChannel', 'RTCPeerConnection',
    //     // 'indexedDB', 'localStorage', 'sessionStorage', 'caches', 'openDatabase', 'StorageManager',
    //     // 'document', 'window', 
    //     // 'alert', 'confirm', 'prompt',
    //     // 'MessageChannel',
    //     // 'WebAssembly',
    //     // 'Function' will be removed because we grabbed a reference to it earlier.
    //     // If we didn't need it, we could add it here.
    // ];

    // sensitiveApis.forEach(apiKey => {
    //     try {
    //         // Check if the API exists (either as own property or inherited)
    //         if (typeof selfWorker[apiKey] !== 'undefined') {
    //             console.log(`[Sandbox Worker] Securing global API: ${apiKey}`);
    //             Object.defineProperty(selfWorker, apiKey, {
    //                 value: undefined, 
    //                 writable: false,
    //                 configurable: false 
    //             });
    //         }
    //     } catch (e) {
    //         console.warn(`[Sandbox Worker] Could not fully secure global API: ${apiKey}`, e.message);
    //     }
    // });

    // Finally, remove the Function constructor from the real global scope.
    // try {
    //     Object.defineProperty(selfWorker, 'Function', {
    //         value: undefined,
    //         writable: false,
    //         configurable: false
    //     });
    // } catch(e) {
    //     console.warn(`[Sandbox Worker] Could not fully secure global API: Function`, e.message);
    // }
}

function loadSource(moduleName, source) {
    console.log("source: ", typeof source);
    // create a wrapper for the imported script
    source = source.replace(/^#!.*?\n/, "");
    const head = "(function (exports, require, module, __filename, __dirname) { (function (exports, require, module, __filename, __dirname) {";
    const foot = "\n}).call(this, exports, require, module, __filename, __dirname); })";
    source = `${head}${source}${foot}`;
    const wrapped = eval(source);

    // create minimal context for the execution
    var module = {
        exports: {}
    };
    // call the script
    wrapped.call(
        module.exports,             // this
        module.exports,             // exports
        require,                    // require
        module,                     // module
        moduleName.substring(1),    // __filename
        moduleName.substring(0,1)   // __dirname
    );

    return module.exports;
}

// prefetch all the requireable scripts before we replace the fetch function
const allowedModules = {
    "~system/BevyExplorerApi": await fetch("modules/systemApi.js").then(async (response) => await response.text()),
    "~system/CommunicationsController": await fetch("modules/CommunicationsController.js").then(async (response) => await response.text()),
    "~system/CommsApi": await fetch("modules/CommsApi.js").then(async (response) => await response.text()),
    "~system/EngineApi": await fetch("modules/EngineApi.js").then(async (response) => await response.text()),
    "~system/EnvironmentApi": await fetch("modules/EnvironmentApi.js").then(async (response) => await response.text()),
    "~system/EthereumController": await fetch("modules/EthereumController.js").then(async (response) => await response.text()),
    "~system/Players": await fetch("modules/Players.js").then(async (response) => await response.text()),
    "~system/PortableExperiences": await fetch("modules/PortableExperiences.js").then(async (response) => await response.text()),
    "~system/RestrictedActions": await fetch("modules/RestrictedActions.js").then(async (response) => await response.text()),
    "~system/Runtime": await fetch("modules/Runtime.js").then(async (response) => await response.text()),
    "~system/Scene": await fetch("modules/Scene.js").then(async (response) => await response.text()),
    "~system/SignedFetch": await fetch("modules/SignedFetch.js").then(async (response) => await response.text()),
    "~system/Testing": await fetch("modules/Testing.js").then(async (response) => await response.text()),
    "~system/UserActionModule": await fetch("modules/UserActionModule.js").then(async (response) => await response.text()),
    "~system/UserIdentity": await fetch("modules/UserIdentity.js").then(async (response) => await response.text()),
    "~system/AdaptationLayerHelper": await fetch("modules/AdaptationLayerHelper.js").then(async (response) => await response.text()),
}

function require(moduleName) {
    let code = allowedModules[moduleName];
    if (!code) {
        throw "can't find module"
    }

    return loadSource(moduleName, code);
}

selfWorker.onmessage = async (event) => {
    if (event.data && event.data.type === 'INIT_WORKER') {
        const { compiledModule, sharedMemory } = event.data.payload;

        if (!compiledModule || !sharedMemory) {
            console.error("[Sandbox Worker] Invalid payload received.");
            return;
        }

        try {
            await init({ module: compiledModule, memory: sharedMemory });
            const context = await wasm_bindgen_exports.wasm_init_scene();

            addDenoOps(wasm_bindgen_exports, context);
            
            // TODO: lock down environment
            // - remove sensitive apis (can we whitelist instead of blacklisting?)
            // - replace fetch, websocket, localstorage

            const sceneCode = context.get_source();
            allowedModules["~scene.js"] = sceneCode;
            let module = require("~scene.js");

            // send any initial rpc requests
            Deno.core.ops.op_crdt_send_to_renderer([]);

            console.log("[Scene Worker] calling onStart");

            await module.onStart();

            console.log("[Scene Worker] entering onUpdate loop");

            var elapsed = 0;
            const startTime = new Date();
            var prevElapsed = 0;
            var count = 0;
            while (true) {
                const runTime = new Date();
                const elapsed = runTime - startTime;
                await module.onUpdate(elapsed - prevElapsed);
                prevElapsed = elapsed;
                count += 1;
            }
        } catch (e) {
            console.error("[Scene Worker] Error during Wasm instantiation or setup:", e);
        }
    }
};

postMessage({ type: `READY`});

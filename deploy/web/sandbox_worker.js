// sandbox_worker.js - Runs inside the final Web Worker, the most isolated environment.

// Import the wasm-bindgen generated JS glue code.
import init, * as wasm_bindgen_exports from "./pkg/webgpu_build.js";

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

const jsContext = Object.create(null);
var jsProxy = undefined;
var jsPreamble = undefined;
function createJsContext(wasmApi, context) {
  Object.defineProperty(jsContext, "console", {
    value: {
      log: console.log.bind(console),
      info: console.log.bind(console),
      debug: console.log.bind(console),
      trace: console.log.bind(console),
      warning: console.error.bind(console),
      error: console.error.bind(console),
    },
  });

  const ops = Object.create(null);
  for (const exportName in wasmApi) {
    // console.log("checking", exportName);
    if (exportName.substring(0, 3) === "op_") {
      // console.log("adding", exportName);
      Object.defineProperty(ops, exportName, {
        configurable: false,
        get() {
          return (...args) => {
            // wrap ops to inject context arg
            return wasmApi[exportName](context, ...args);
          };
        },
      });
    }
  }
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
    value: createWebStorageProxy(ops)
  })

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
    `${jsPreamble}\n\n${code}`
  );

  await defer(() => func.call(jsProxy, jsProxy, module, module.exports));
  return module.exports;
}

// prefetch all the requireable scripts before we replace the fetch function
var allowedModules = undefined;

async function preloadModules() {
  const modules = {
    "~system/BevyExplorerApi": "modules/systemApi.js",
    "~system/CommunicationsController": "modules/CommunicationsController.js",
    "~system/CommsApi": "modules/CommsApi.js",
    "~system/EngineApi": "modules/EngineApi.js",
    "~system/EnvironmentApi": "modules/EnvironmentApi.js",
    "~system/EthereumController": "modules/EthereumController.js",
    "~system/Players": "modules/Players.js",
    "~system/PortableExperiences": "modules/PortableExperiences.js",
    "~system/RestrictedActions": "modules/RestrictedActions.js",
    "~system/Runtime": "modules/Runtime.js",
    "~system/Scene": "modules/Scene.js",
    "~system/SignedFetch": "modules/SignedFetch.js",
    "~system/Testing": "modules/Testing.js",
    "~system/UserActionModule": "modules/UserActionModule.js",
    "~system/UserIdentity": "modules/UserIdentity.js",
    "~system/AdaptationLayerHelper": "modules/AdaptationLayerHelper.js",
  };

  const promises = Object.entries(modules).map(async ([key, path]) => {
    const response = await fetch(path);
    const code = await response.text();
    const result = await runWithScope(code);
    return [key, result];
  });

  allowedModules = Object.fromEntries(await Promise.all(promises));
}

function require(moduleName) {
  let code = allowedModules[moduleName];
  if (!code) {
    // console.log(allowedModules);
    throw "can't find module `" + moduleName + "`";
  }

  return code;
}

self.onmessage = async (event) => {
  if (event.data && event.data.type === "INIT_WORKER") {
    const { compiledModule, sharedMemory } = event.data.payload;

    if (!compiledModule || !sharedMemory) {
      console.error("[Sandbox Worker] Invalid payload received.");
      return;
    }

    try {
      // init wasm
      await init({ module: compiledModule, memory: sharedMemory });
      const wasmContext = await wasm_bindgen_exports.wasm_init_scene();
      createJsContext(wasm_bindgen_exports, wasmContext);
      const ops = jsContext.Deno.core.ops;

      // preload modules
      await preloadModules();

      const sceneCode = wasmContext.get_source();
      let module = await runWithScope(sceneCode);

      // send any initial rpc requests
      ops.op_crdt_send_to_renderer([]);

      console.log("[Scene Worker] calling onStart");
      await module.onStart();

      console.log("[Scene Worker] entering onUpdate loop");

      var elapsed = 0;
      const startTime = new Date();
      var prevElapsed = 0;
      var elapsed = 0;
      var count = 0;
      while (ops.op_continue_running()) {
        await module.onUpdate(elapsed - prevElapsed);
        prevElapsed = elapsed;
        elapsed = new Date() - startTime;
        count += 1;
      }
      console.log("[Scene Worker] exiting gracefully");
    } catch (e) {
      console.error(
        "[Scene Worker] Error during Wasm instantiation or setup:",
        e
      );
    }
  }
};

function createWebStorageProxy(ops) {
  return new Proxy({}, {
    get(_target, prop, _receiver) {
      if (prop === 'length') {
        return ops.op_webstorage_length();
      }

      if (prop === 'getItem') {
        return (key) => ops.op_webstorage_get(String(key));
      }
      if (prop === 'setItem') {
        return (key, value) => ops.op_webstorage_set(String(key), String(value));
      }
      if (prop === 'key') {
        return (index) => ops.op_webstorage_key(index);
      }
      if (prop === 'removeItem') {
        return (key) => ops.op_webstorage_remove(String(key));
      }
      if (prop === 'clear') {
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
        }
      }
      return undefined;
    },

    has(_target, prop) {
      return ops.op_webstorage_has(String(prop))
    }
  });
}

postMessage({ type: `READY` });

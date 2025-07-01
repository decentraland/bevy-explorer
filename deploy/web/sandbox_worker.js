// sandbox_worker.js - Runs inside the final Web Worker, the most isolated environment.

// Import the wasm-bindgen generated JS glue code.
import init, * as wasm_bindgen_exports from "./pkg/webgpu_build.js";

// self.WebSocket = {}

console.log("[Scene Worker] Starting");

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
  const isSuper = wasmApi.is_super(context);

  Object.defineProperty(jsContext, "console", {
    value: {
      log: console.log.bind(console),
      info: console.info.bind(console),
      debug: console.debug.bind(console),
      trace: console.trace.bind(console),
      warning: console.error.bind(console),
      error: console.error.bind(console),
    },
  });

  const ops = Object.create(null);
  for (const exportName in wasmApi) {
    if (exportName.substring(0, 3) === "op_") {
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
    `${jsPreamble}\n\n${code}`
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

self.onmessage = async (event) => {
  if (event.data && event.data.type === "INIT_WORKER") {
    const { compiledModule, sharedMemory } = event.data.payload;

    if (!compiledModule || !sharedMemory) {
      console.error("[Sandbox Worker] Invalid payload received.");
      return;
    }

    var wasm_init;
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
      postMessage({ type: `INIT_FAILED` });
      self.close();
      return;
    }

    postMessage({ type: `INIT_COMPLETE` });
        
    // add listener to clean up on unhandled rejections
    self.addEventListener("unhandledrejection", (event) => {
      // Prevent the default browser action (logging to console)
      event.preventDefault();

      console.error(
        "[Sandbox worker] FATAL: Unhandled Promise Rejection in Worker:",
        event.reason
      );

      try {
        wasm_init.__wbindgen_thread_destroy();
      } catch (cleanupError) {
        console.error(
          "[Sandbox worker] Error during WASM cleanup:",
          cleanupError
        );
      }

      self.close();
    });

    var wasmContext;
    try {
      wasmContext = await wasm_bindgen_exports.wasm_init_scene();
    } catch (e) {
      console.error("[Scene Worker] Error during scene construction:", e);
      try {
        wasm_init.drop_context(wasmContext);
      } catch (e) {}
      wasm_init.__wbindgen_thread_destroy();
      self.close();
      return;
    }

    try {
      createJsContext(wasm_bindgen_exports, wasmContext);
      const ops = jsContext.Deno.core.ops;

      // preload modules
      await preloadModules(wasmContext, wasm_bindgen_exports.builtin_module);

      const sceneCode = wasmContext.get_source();
      let module = await runWithScope(sceneCode);

      // send any initial rpc requests
      ops.op_crdt_send_to_renderer([]);

      await module.onStart();

      var elapsed = 0;
      const startTime = new Date();
      var prevElapsed = 0;
      var elapsed = 0;
      var count = 0;
      while (ops.op_continue_running()) {
        const dt = (elapsed - prevElapsed) / 1000;
        await module.onUpdate(dt);
        prevElapsed = elapsed;
        elapsed = new Date() - startTime;
        count += 1;
      }
      console.log("[Scene Worker] exiting gracefully");
    } catch (e) {
      console.error("[Scene Worker] Error during scene execution:", e);
    }

    try {
      wasm_init.drop_context(wasmContext);
    } catch (e) {}
    wasm_init.__wbindgen_thread_destroy();
    self.close();
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

postMessage({ type: `READY` });

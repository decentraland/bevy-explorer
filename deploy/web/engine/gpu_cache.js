var count = 0;

function simpleHash(s) {
  var h = 0x811c9dc5;

  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h += (h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24);
  }

  return h >>> 0;
}

var gpuSessionState = {};
var requiredItemTypes = new Map();
var precaching = false;

const dbConfig = {
  name: "GpuCacheDB",
  version: 1,
  onUpgrade: (db) => {
    if (!db.objectStoreNames.contains("shader")) db.createObjectStore("shader");
    if (!db.objectStoreNames.contains("bindgroup"))
      db.createObjectStore("bindgroup");
    if (!db.objectStoreNames.contains("layout")) db.createObjectStore("layout");
    if (!db.objectStoreNames.contains("pipeline"))
      db.createObjectStore("pipeline");
    if (!db.objectStoreNames.contains("requiredItems"))
      db.createObjectStore("requiredItems");
    if (!db.objectStoreNames.contains("deviceConfig"))
      db.createObjectStore("deviceConfig");
  },
};

var dbPromise;
function openDB() {
  if (dbPromise) {
    return dbPromise;
  }
  dbPromise = new Promise((resolve, reject) => {
    const request = indexedDB.open(dbConfig.name, dbConfig.version);
    request.onupgradeneeded = (event) => {
      dbConfig.onUpgrade(event.target.result);
    };
    request.onsuccess = (event) => resolve(event.target.result);
    request.onerror = (event) => reject(event.target.error);
  });
  return dbPromise;
}

async function clearDatabase() {
  const db = await openDB();

  const tableNames = ["shader", "bindgroup", "layout", "pipeline", "requiredItems"];

  const tx = db.transaction(tableNames, "readwrite");

  const clearPromises = tableNames.map(name => {
    const store = tx.objectStore(name);
    const request = store.clear();
    return new Promise((resolve, reject) => {
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  });

  await Promise.all(clearPromises);
  await new Promise((resolve, reject) => {
    tx.oncomplete = resolve;
    tx.onabort = reject;
    tx.onerror = reject;
  });
}

async function fetchRequiredItems() {
  const db = await openDB();
  return new Promise((resolve) => {
    const tx = db.transaction("requiredItems", "readonly");
    const request = tx.objectStore("requiredItems").get("it");
    request.onsuccess = () => {
      resolve(request.result ?? new Map());
    };
    request.onerror = () => {
      resolve(new Map());
    };
  });
}

async function storeRequiredItems() {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("requiredItems", "readwrite");
    tx.oncomplete = () => {
      resolve();
    };
    tx.onerror = () => {
      console.log("[GPU Cache] failed to store requiredItems");
      reject(tx.error);
    };
    tx.objectStore("requiredItems").put(requiredItemTypes, "it");
  });
}

// The small settings that lived in localStorage, which a worker has no access to.
async function getConfig(key) {
  const db = await openDB();
  return new Promise((resolve) => {
    const request = db.transaction("deviceConfig", "readonly").objectStore("deviceConfig").get(key);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => resolve(undefined);
  });
}

async function setConfig(key, value) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("deviceConfig", "readwrite");
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.objectStore("deviceConfig").put(value, key);
  });
}

async function fetchInstance(type, hash) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(type, "readonly");
    const request = tx.objectStore(type).get(hash);
    request.onsuccess = () => {
      if (request.result) {
        resolve(JSON.parse(request.result));
      } else {
        reject("undefined");
      }
    };
    request.onerror = () => {
      reject("error");
    };
  });
}

async function storeInstance(type, hash, value) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(type, "readwrite");
    tx.oncomplete = () => {
      resolve();
    };
    tx.onerror = () => {
      console.log("[GPU Cache] failed to store ${type} instance");
      reject(tx.error);
    };
    tx.objectStore(type).put(JSON.stringify(value), hash);
  });
}

export async function initGpuCache(key, fakeAsync) {
  console.log(`[GPU Cache] key: ${key}`);
  patchWebgpuAdapter(fakeAsync);
  await createGpuCache(key);
}

function patchWebgpuAdapter(fakeAsync) {
  const originalRequestDevice = GPUAdapter.prototype.requestDevice;
  GPUAdapter.prototype.requestDevice = async function (descriptor) {
    const jsonDescriptor = JSON.stringify(descriptor || {});
    if (gpuSessionState.deviceDescriptor === jsonDescriptor) {
      console.log("[GPU Cache] using precached device");
      return gpuSessionState.device;
    }

    if (!precaching) {
      console.log(
        "[GPU Cache] device params are different: creating device and clearing cache"
      );
      clearDatabase();
    }
    gpuSessionState = {
      shader: new Map(),
      bindgroup: new Map(),
      layout: new Map(),
      pipeline: new Map(),
    };
    gpuSessionState.deviceDescriptor = jsonDescriptor;
    var device = undefined;
    try {
      device = await originalRequestDevice.apply(this, [descriptor]);
    } catch (err) {
      console.warn("Failed to request a device with descriptor, requesting device with default parameters.");
      device = await originalRequestDevice.apply(this, []);
    }
    gpuSessionState.device = device;
    await setConfig("deviceDescriptor", jsonDescriptor);

    function wrapDeviceFunction(itemType, originalFunction) {
      return (...args) => {
        const jsonArgs = JSON.stringify(args);
        const hash = simpleHash(jsonArgs);
        const cachedItem = gpuSessionState[itemType].get(hash);
        if (cachedItem !== undefined) {
          return cachedItem;
        }

        const item = originalFunction.apply(device, args);
        item.__gpu_item_type = itemType;
        item.__gpu_hash = hash;
        gpuSessionState[itemType].set(hash, item);

        if (!precaching) {
          console.log(`[GPU Cache] no cached ${itemType} for ${hash}`);
          if (!requiredItemTypes.has(itemType)) {
            requiredItemTypes.set(itemType, new Set());
          }
          const requiredItems = requiredItemTypes.get(itemType);
          if (!requiredItems.has(hash)) {
            requiredItems.add(hash);
            storeRequiredItems();
            storeInstance(itemType, hash, args);
          }
        }
        return item;
      };
    }

    device.createShaderModule = wrapDeviceFunction(
      "shader",
      device.createShaderModule
    );
    device.createBindGroupLayout = wrapDeviceFunction(
      "bindgroup",
      device.createBindGroupLayout
    );
    device.createPipelineLayout = wrapDeviceFunction(
      "layout",
      device.createPipelineLayout
    );
    if (fakeAsync) {
      device.createRenderPipeline = wrapDeviceFunction(
        "pipeline",
        device.createRenderPipeline
      );
    } else {
      let inline_function = wrapDeviceFunction("pipeline", device.createRenderPipeline);

      globalThis.pendingAsyncPipelineCount = 0;
      globalThis.lastPipelineWasValidFlag = false;
      globalThis.wgpuResolveIdle = [];
      const itemType = "pipeline";
      const placeholderPipeline = getPlaceholder(device);

      device.createRenderPipeline = (...args) => {
        const jsonArgs = JSON.stringify(args);
        const hash = simpleHash(jsonArgs);
        const cachedItem = gpuSessionState[itemType].get(hash);
        if (cachedItem !== undefined) {
          globalThis.nextPipelineCanFail = false;
          globalThis.lastPipelineWasValidFlag = true;
          return cachedItem;
        }

        console.log(`[GPU Cache] (async) no cached ${itemType} for ${hash}`);

        if (!globalThis.nextPipelineCanFail) {
          return inline_function.apply(device, args);
        }
        globalThis.__setShaderCompiling?.(true);
        globalThis.nextPipelineCanFail = false;
        globalThis.lastPipelineWasValidFlag = false;
        globalThis.pendingAsyncPipelineCount++;

        const promise = device.createRenderPipelineAsync(args[0]).then(async (item) => {
          item.__gpu_item_type = itemType;
          item.__gpu_hash = hash;
          gpuSessionState[itemType].set(hash, item);
          globalThis.pendingAsyncPipelineCount--;

          if (!requiredItemTypes.has(itemType)) {
            requiredItemTypes.set(itemType, new Set());
          }
          const requiredItems = requiredItemTypes.get(itemType);
          if (!requiredItems.has(hash)) {
            requiredItems.add(hash);
            await storeRequiredItems();
            await storeInstance(itemType, hash, args);
          }

          if (globalThis.pendingAsyncPipelineCount === 0) {
            while (globalThis.wgpuResolveIdle.length > 0) {
              globalThis.wgpuResolveIdle.pop()();
            }
            globalThis.__setShaderCompiling?.(false);
          }

          return item;
        })

        return placeholderPipeline;
      }
    }

    return device;
  };
}

async function createGpuCache(key) {
  const cachedKey = await getConfig("gpuCacheKey");
  if (cachedKey != key) {
    console.log("shaders updated, clearing db");
    await clearDatabase();
    await setConfig("gpuCacheKey", key);
    return;
  }

  const cachedDeviceDescriptor = await getConfig("deviceDescriptor");
  if (cachedDeviceDescriptor === undefined) {
    return;
  }
  precaching = true;
  const started = performance.now();
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice(
    JSON.parse(cachedDeviceDescriptor)
  );

  requiredItemTypes = await fetchRequiredItems();
  try {
    await createItemType("shader", async (args) => {
      return device.createShaderModule(args);
    });
    await createItemType("bindgroup", async (args) => {
      return device.createBindGroupLayout(args);
    });
    await createItemType("layout", async (args) => {
      return device.createPipelineLayout(args);
    });
    await createItemType("pipeline", async (args) => {
      return await device.createRenderPipelineAsync(args);
    });
  } finally {
    precaching = false;
  }
  storeRequiredItems();
  const stats = Object.keys(gpuSessionState).map(
    (k) => `\n${k}: ${gpuSessionState[k].size ?? "ok"}`
  );
  console.log(`[GPU Cache]: preloaded in ${Math.round(performance.now() - started)} ms ${stats}`);
}

async function createItemType(itemType, asyncCreateFunction) {
  const storedItems = requiredItemTypes.get(itemType) ?? new Set();

  await Promise.all(
    [...storedItems].map(async (hash) => {
      try {
        const args = await fetchInstance(itemType, hash);
        rehydrateItem(args);
        const item = await asyncCreateFunction(args[0]);
        gpuSessionState[itemType].set(hash, item);
      } catch (e) {
        console.warn(`[GPU Cache] failed to precreate ${itemType}: ${e}`);
        storedItems.delete(hash);
      }
    })
  );
}

function rehydrateItem(currentObject) {
  if (typeof currentObject !== "object" || currentObject === null) {
    return;
  }

  if (Array.isArray(currentObject)) {
    for (let i = 0; i < currentObject.length; i++) {
      const item = currentObject[i];
      if (item && item.__gpu_hash) {
        const type = item.__gpu_item_type;
        const hash = item.__gpu_hash;
        currentObject[i] = gpuSessionState[type].get(hash);
        if (!currentObject[i]) {
          throw `failed to rehydrate: missing ${type} with hash ${hash}`;
        }
      } else {
        rehydrateItem(item);
      }
    }
  } else {
    for (const key in currentObject) {
      if (Object.prototype.hasOwnProperty.call(currentObject, key)) {
        const value = currentObject[key];
        if (value && value.__gpu_hash) {
          const type = value.__gpu_item_type;
          const hash = value.__gpu_hash;
          currentObject[key] = gpuSessionState[type].get(hash);
          if (!currentObject[key]) {
            throw `failed to rehydrate: missing ${type} with hash ${hash}`;
          }
        } else {
          rehydrateItem(value);
        }
      }
    }
  }
}

function getPlaceholder(device) {
  // create a trivial shader and pipeline
  const module = device.createShaderModule({
    code: `
        @vertex fn vs_main() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }
        @fragment fn fs_main() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
        `
  });

  return device.createRenderPipeline({
    layout: device.createPipelineLayout({
      bindGroupLayouts: []
    }),
    vertex: { module, entryPoint: 'vs_main' },
    fragment: { module, entryPoint: 'fs_main', targets: [{ format: 'bgra8unorm' }] }, // Adjust format if needed
    primitive: { topology: 'triangle-list' }
  });
}

globalThis.allowADummyPipeline = function() {
    globalThis.nextPipelineCanFail = true;
}

globalThis.lastPipelineWasValid = function() {
    return globalThis.lastPipelineWasValidFlag;
}

globalThis.waitForPipelines = function() {
    if (globalThis.pendingAsyncPipelineCount === 0) {
        return Promise.resolve();
    }
    return new Promise((resolve) => {
        globalThis.wgpuResolveIdle.push(resolve);
    });
};
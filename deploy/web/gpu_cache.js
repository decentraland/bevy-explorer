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

async function clearDatabase() {
  const storeNames = ["shader", "bindgroup", "layout", "pipeline"];

  const tx = await db.transaction("rqeuiredItems", "readwrite");
  await tx.objectStore("requiredItems").clear();

  const clearPromises = storeNames.map((name) => {
    const tx = db.transaction(name, "readwrite");
    return new Promise((resolve, reject) => {
      const request = tx.objectStore(name).clear();
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  });

  // Wait for all clear operations to complete
  await Promise.all(clearPromises);
  await tx.done;
}

export async function initGpuCache() {
  patchWebgpuAdater();
  await createGpuCache();
}

function patchWebgpuAdater() {
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
    const device = await originalRequestDevice.apply(this, [descriptor]);
    gpuSessionState.device = device;
    localStorage.setItem("deviceDescriptor", jsonDescriptor);

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
    device.createRenderPipeline = wrapDeviceFunction(
      "pipeline",
      device.createRenderPipeline
    );

    return device;
  };
}

async function createGpuCache() {
  const cachedDeviceDescriptor = localStorage.getItem("deviceDescriptor");
  if (cachedDeviceDescriptor === null) {
    return;
  }
  precaching = true;
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
  console.log(`[GPU Cache]: preloaded ${stats}`);
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

/**
 * JS Implementation of MurmurHash3 (128-bit)
 * @author <a href="mailto:gary.court@gmail.com">Gary Court</a>
 * @see http://github.com/garycourt/murmurhash-js
 * @author <a href="mailto:aappleby@gmail.com">Austin Appleby</a>
 * @see http://sites.google.com/site/murmurhash/
 */
function murmur3_128(key) {
  let h1 = 0x9747b28c,
    h2 = 0x9747b28c,
    h3 = 0x9747b28c,
    h4 = 0x9747b28c;
  const len = key.length;

  for (let i = 0; i < len; i++) {
    let k1 = key.charCodeAt(i);
    k1 =
      (k1 & 0xffff) * 0xcc9e2d51 +
      ((((k1 >>> 16) * 0xcc9e2d51) & 0xffff) << 16);
    k1 = (k1 << 15) | (k1 >>> 17);
    k1 =
      (k1 & 0xffff) * 0x1b873593 +
      ((((k1 >>> 16) * 0x1b873593) & 0xffff) << 16);

    h1 ^= k1;
    h1 = (h1 << 19) | (h1 >>> 13);
    h1 = h1 * 5 + 0x561ccd1b;
  }

  h1 ^= len;
  h2 ^= len;
  h3 ^= len;
  h4 ^= len;
  h1 += h2;
  h1 += h3;
  h1 += h4;
  h2 += h1;
  h3 += h1;
  h4 += h1;

  h1 ^= h1 >>> 16;
  h1 =
    (h1 & 0xffff) * 0x85ebca6b + ((((h1 >>> 16) * 0x85ebca6b) & 0xffff) << 16);
  h1 ^= h1 >>> 13;
  h1 =
    (h1 & 0xffff) * 0xc2b2ae35 + ((((h1 >>> 16) * 0xc2b2ae35) & 0xffff) << 16);
  h1 ^= h1 >>> 16;

  // Convert to hex string for the final key
  return h1.toString(16);
}

var gpuSessionState = {};
var requiredItemTypes = new Map();
var precaching = false;

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

    console.log("[GPU Cache] creating device and clearing cache");
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
        const hash = murmur3_128(jsonArgs);
        const cachedItem = gpuSessionState[itemType].get(hash);
        if (cachedItem !== undefined) {
          console.log(`[GPU Cache] using cached ${itemType} ${hash}`);
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
            localStorage.setItem(
              `required-${itemType}s`,
              JSON.stringify([...requiredItems])
            );
            localStorage.setItem(`${itemType}-${hash}`, JSON.stringify(args));
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
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice(
    JSON.parse(cachedDeviceDescriptor)
  );

  precaching = true;
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
}

async function createItemType(itemType, asyncCreateFunction) {
  const storedString = localStorage.getItem(`required-${itemType}s`);
  const storedItems = storedString ? JSON.parse(storedString) : [];
  const requiredItems = new Set();

  await Promise.all(
    storedItems.map(async (hash) => {
      requiredItems.add(hash);
      const argString = localStorage.getItem(`${itemType}-${hash}`);
      if (!argString) {
        return;
      }
      const args = JSON.parse(argString);
      try {
        rehydrateItem(args);
        const item = await asyncCreateFunction(args[0]);
        gpuSessionState[itemType].set(hash, item);
      } catch (e) {
        console.warn(`[GPU Cache] failed to precreate ${itemType}: ${e}`);
      }
    })
  );

  requiredItemTypes.set(itemType, requiredItems);
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

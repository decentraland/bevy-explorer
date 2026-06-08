// Save a scene composite to the user's real filesystem on the web, via the File System Access
// API. A FileSystemDirectoryHandle can't be byte-serialized (so not OPFS/localStorage), but it is
// structured-cloneable, so we persist it in IndexedDB keyed by the scene id and reuse it to skip
// the directory picker on subsequent saves.

const DB_NAME = 'dcl-editor'
const STORE = 'scene-dirs'

function openDb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE)
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function idbGet(key) {
  const db = await openDb()
  try {
    return await new Promise((resolve, reject) => {
      const req = db.transaction(STORE, 'readonly').objectStore(STORE).get(key)
      req.onsuccess = () => resolve(req.result)
      req.onerror = () => reject(req.error)
    })
  } finally {
    db.close()
  }
}

async function idbPut(key, value) {
  const db = await openDb()
  try {
    await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, 'readwrite')
      tx.objectStore(STORE).put(value, key)
      tx.oncomplete = () => resolve()
      tx.onerror = () => reject(tx.error)
    })
  } finally {
    db.close()
  }
}

async function hasPermission(handle) {
  const opts = { mode: 'readwrite' }
  if ((await handle.queryPermission(opts)) === 'granted') return true
  return (await handle.requestPermission(opts)) === 'granted'
}

// key: scene id (cache key); relPath: e.g. "assets/scene/main.composite"; bytes: Uint8Array.
// Returns the written relative path.
export async function saveComposite(key, relPath, bytes) {
  // The wasm memory is a SharedArrayBuffer (atomics build), and `bytes` is a view over it;
  // FileSystemWritableFileStream.write() rejects views over shared memory, so copy into a plain
  // ArrayBuffer first (also keeps the data stable across the awaits below).
  const data = new Uint8Array(bytes)

  let root = await idbGet(key)
  if (root && !(await hasPermission(root))) root = null
  if (!root) {
    root = await window.showDirectoryPicker({ id: 'dcl-scene', mode: 'readwrite' })
    if (!(await hasPermission(root))) throw new Error('permission denied')
    await idbPut(key, root)
  }

  const parts = relPath.split('/')
  const name = parts.pop()
  let dir = root
  for (const part of parts) dir = await dir.getDirectoryHandle(part, { create: true })
  const file = await dir.getFileHandle(name, { create: true })
  const writable = await file.createWritable()
  await writable.write(data)
  await writable.close()
  return `${root.name}/${relPath}`
}

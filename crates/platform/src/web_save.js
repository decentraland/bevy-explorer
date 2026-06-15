// Save scene files (the composite + imported assets) to the user's real filesystem on web, via the
// File System Access API. The user grants ONE directory (a workspace, or a specific project folder)
// and we remember its handle in IndexedDB (a FileSystemDirectoryHandle is structured-cloneable).
//
// We never trust the granted handle blindly: the engine passes the scene's identity — its absolute
// project root (for navigation) plus a fingerprint (projectId, else parcels+title) — and we locate
// the scene's project folder *under* the handle by walking the root path as suffixes (the handle may
// be the project root or an ancestor of it) and matching scene.json. A handle that doesn't contain
// the scene is rejected and the user is re-prompted, so we can't write into the wrong project.

const DB_NAME = 'dcl-editor'
const DB_VERSION = 2
const STORE = 'handles'
const HANDLE_KEY = 'scene-dir'

function openDb() {
  return new Promise((resolve, reject) => {
    // Bump DB_VERSION whenever STORE changes — onupgradeneeded only fires on a version increase, so
    // an existing DB (e.g. from the old per-scene store) needs this to gain the current store.
    const req = indexedDB.open(DB_NAME, DB_VERSION)
    req.onupgradeneeded = () => {
      const db = req.result
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE)
    }
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

// Navigate `segs` (directory names) down from `handle`; null if any segment is missing.
async function getDir(handle, segs) {
  let dir = handle
  for (const seg of segs) {
    dir = await dir.getDirectoryHandle(seg, { create: false })
  }
  return dir
}

async function readSceneJson(dir) {
  try {
    const fh = await dir.getFileHandle('scene.json', { create: false })
    return JSON.parse(await (await fh.getFile()).text())
  } catch {
    return null
  }
}

// Does `sceneJson` identify the scene the engine described? projectId is authoritative; fall back to
// parcels + title for scenes without a Creator-Hub `source` block.
function sceneMatches(sceneJson, target) {
  if (!sceneJson) return false
  const projectId = sceneJson.source && sceneJson.source.projectId
  if (target.projectId && projectId) return projectId === target.projectId
  const parcels = (sceneJson.scene && sceneJson.scene.parcels) || []
  const wantParcels = target.parcels || []
  const parcelsMatch =
    wantParcels.length > 0 &&
    wantParcels.length === parcels.length &&
    wantParcels.every((p) => parcels.includes(p))
  const title = sceneJson.display && sceneJson.display.title
  return parcelsMatch && (!target.title || target.title === title)
}

// Find the scene's project-root directory handle under `handle`, by trying the engine's absolute
// project path as suffixes — handle == project root (empty suffix) first, then handle == its parent,
// grandparent, … — and matching scene.json. Returns the dir handle, or null.
async function findProjectDir(handle, target) {
  const segs = (target.root || '').split('/').filter(Boolean)
  for (let i = segs.length; i >= 0; i--) {
    let dir
    try {
      dir = await getDir(handle, segs.slice(i))
    } catch {
      continue
    }
    if (sceneMatches(await readSceneJson(dir), target)) return dir
  }
  return null
}

async function pickDir() {
  const handle = await window.showDirectoryPicker({ id: 'dcl-scene', mode: 'readwrite' })
  if (!(await hasPermission(handle))) throw new Error('permission denied')
  await idbPut(HANDLE_KEY, handle)
  return handle
}

// Resolved project-root dir handles for this session, keyed by scene fingerprint — so the folder is
// located/verified once, not on every file of a multi-file save.
const resolved = new Map()

// scene_target: JSON `{root, projectId, parcels, title}`; relPath: e.g. "assets/scene/main.composite";
// bytes: Uint8Array. Returns the written relative path.
export async function saveSceneFile(sceneTarget, relPath, bytes) {
  const target = JSON.parse(sceneTarget)
  // The wasm memory is a SharedArrayBuffer (atomics build) and `bytes` views it;
  // FileSystemWritableFileStream.write() rejects views over shared memory, so copy first.
  const data = new Uint8Array(bytes)

  const cacheKey = target.projectId || target.root || ''
  let projectDir = resolved.get(cacheKey)

  if (!projectDir) {
    let handle = await idbGet(HANDLE_KEY)
    if (handle && !(await hasPermission(handle))) handle = null
    if (handle) projectDir = await findProjectDir(handle, target)
    if (!projectDir) {
      // first save, or the remembered folder doesn't contain this scene — ask for one
      handle = await pickDir()
      projectDir = await findProjectDir(handle, target)
    }
    if (!projectDir) {
      throw new Error("chosen folder isn't this scene's project (no matching scene.json)")
    }
    resolved.set(cacheKey, projectDir)
  }

  const parts = relPath.split('/')
  const name = parts.pop()
  let dir = projectDir
  for (const part of parts) {
    dir = await dir.getDirectoryHandle(part, { create: true })
  }
  const file = await dir.getFileHandle(name, { create: true })
  const writable = await file.createWritable()
  await writable.write(data)
  await writable.close()
  return `${projectDir.name}/${relPath}`
}

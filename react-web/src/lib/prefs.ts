// The HUD's own small persisted values (UI choices, recents, caches), and the only place the HUD
// stores anything. Inside the web shell the app must never touch localStorage: reading it loads the
// origin's whole storage area into the app's process, and that area holds the sign-in key the shell
// keeps (src/shell/main.ts). There they live in IndexedDB, read once before the HUD renders and
// written through. The native CEF HUD does the same against the engine (hud-prefs.json beside
// config.json), as CEF's own storage is gone when the client exits. Elsewhere (the credentialless
// embed, app.html opened directly in dev) in localStorage.
import { cefPrefs } from './cefNativeBridge'
import { inShell } from './shell'

// Every key the HUD stores. The shell moves these out of localStorage (migratePrefs).
export const PREF = {
  emojiRecents: 'dcl-emoji-recents',
  lobbyRecents: 'lobby.recentPlaces',
  minimapPlaces: 'dcl-minimap-places',
  minimapStyle: 'dcl-minimap-style',
  minimapRotation: 'dcl-minimap-rotation',
  minimapZoom: 'dcl-minimap-zoom',
  minimapMarkers: 'dcl-minimap-markers',
  minimapOpen: 'dcl-minimap-open',
  minimapWorldOpen: 'dcl-minimap-open-world',
  loadingLastTip: 'loadingLastTip',
  sidebarAutoHide: 'hud.sidebarAutoHide',
  voiceDisabled: 'nearbyVoiceDisabled',
  voiceVolume: 'nearbyVoiceVolume',
  voiceUsed: 'nearbyVoiceUsed',
  launchCount: 'launchCount'
} as const

const DB = 'hud-prefs'
const STORE = 'prefs'
const values = new Map<string, string>()
const engine = cefPrefs()

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE)
    req.onsuccess = () => {
      // a newer version can't upgrade the database while this connection is open
      req.result.onversionchange = () => req.result.close()
      resolve(req.result)
    }
    req.onerror = () => reject(req.error)
    req.onblocked = () => reject(new Error('blocked by an open older version'))
  })
}

let db: Promise<IDBDatabase> | null = null
function store(mode: IDBTransactionMode): Promise<IDBObjectStore> {
  db ??= openDb()
  return db.then((d) => d.transaction(STORE, mode).objectStore(STORE))
}

// Read every stored value; the HUD renders after this. A failure leaves the defaults.
export async function loadPrefs(): Promise<void> {
  if (engine != null) {
    for (const [key, value] of Object.entries(await engine.load())) {
      if (typeof value === 'string') values.set(key, value)
    }
    return
  }
  if (!inShell) return
  try {
    const s = await store('readonly')
    await new Promise<void>((resolve, reject) => {
      const req = s.openCursor()
      req.onsuccess = () => {
        const cursor = req.result
        if (!cursor) return resolve()
        if (typeof cursor.value === 'string') values.set(String(cursor.key), cursor.value)
        cursor.continue()
      }
      req.onerror = () => reject(req.error)
    })
  } catch (e) {
    console.warn('[prefs] could not read stored preferences:', e)
  }
}

export function getPref(key: string): string | null {
  if (inShell || engine != null) return values.get(key) ?? null
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

// Failures (quota, privacy mode, no IndexedDB) are ignored: the value just lasts this session.
export function setPref(key: string, value: string): void {
  if (engine != null) {
    values.set(key, value)
    engine.set(key, value)
    return
  }
  if (!inShell) {
    try {
      localStorage.setItem(key, value)
    } catch {}
    return
  }
  values.set(key, value)
  store('readwrite')
    .then((s) => s.put(value, key))
    .catch((e: unknown) => console.warn('[prefs] could not store', key, e))
}

// Shell side, before the app loads: move any values an earlier version kept in localStorage into
// IndexedDB, and drop them from localStorage.
export async function migratePrefs(): Promise<void> {
  const found: [string, string][] = []
  for (const key of Object.values(PREF)) {
    const value = localStorage.getItem(key)
    if (value != null) found.push([key, value])
  }
  if (found.length === 0) return
  const s = await store('readwrite')
  await new Promise<void>((resolve, reject) => {
    for (const [key, value] of found) s.put(value, key)
    s.transaction.oncomplete = () => resolve()
    s.transaction.onerror = () => reject(s.transaction.error)
    s.transaction.onabort = () => reject(s.transaction.error ?? new Error('aborted'))
  })
  for (const [key] of found) localStorage.removeItem(key)
}

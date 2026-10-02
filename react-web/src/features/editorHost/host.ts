// The page's side of the editor host contract (v0): the object the editor package finds at
// window.__dclEditorHost, the element it mounts into, and the script tag that loads it.

import { bridgeChannelName } from '../../engine/protocol'
import { DEFAULT_REALM } from '../../lib/baseDomain'
import type { EditorSource } from './config'
import type { EditorHudMode } from './hudMode'

export interface DclEditorHostV0 {
  version: 0
  /** Directory url of this page (the service worker's scope). */
  pageDir: string
  /** Directory url the editor package is served from. */
  editorBase: string
  /** The per-tab BroadcastChannel suffix; page-side code appends `#<busSession>` itself. */
  busSession: string
  /** Covers the viewport, click-through, above the canvas: the editor mounts inside it. */
  container: HTMLElement
  engineConsole: (line: string) => Promise<string>
  identity: () => { address: string | null; isGuest: boolean }
  setMode: (mode: 'edit' | 'play' | 'off') => void
  /** Travel the running engine to `<pageDir>preview/<projectId>` at parcel `x,y`. */
  openPreview: (projectId: string, position: string) => Promise<void>
  /** Spawn the package's own scene (`<editorBase>scene`) with its permissions; resolves once live. */
  spawnEditorScene: () => Promise<{ hash: string }>
  /** Unmount the editor, kill its scene, restore the HUD and travel back. */
  exit: () => void
}

/** What the host needs from the session, read at call time. */
export interface EditorHostDeps {
  engineConsole: (line: string) => Promise<string>
  identity: () => { address: string | null; isGuest: boolean }
  setMode: (mode: EditorHudMode) => void
  travel: (realm: string, parcel?: { x: number; y: number }) => Promise<void>
  scene: (action: 'spawn' | 'kill', source: string, hash: string) => Promise<void>
}

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV0
  __dclEditor?: { unmount: () => void }
  __bridgeSession?: string
  set_url_params?: (optionsJson: string) => void
}

interface Home {
  realm: string | null
  position: string | null
}

// deploy/web/PREVIEW_REALM.md
const PROJECT_ID = /^[a-z0-9][a-z0-9-]{0,63}$/
const PARCEL = /^(-?\d+),(-?\d+)$/

// Where the player was before the first preview.
let home: Home | null = null

/** The engine's url-sync options as the page records them: `editor` kept (the engine echoes its
 *  own, unset, flag) and a preview realm replaced by the place to go back to. */
function hostUrlOptions(optionsJson: string, previewRoot: string, back: Home | null): string {
  const options = JSON.parse(optionsJson) as Record<string, unknown>
  options.editor = true
  if (typeof options.realm === 'string' && options.realm.startsWith(previewRoot)) {
    options.realm = back?.realm ?? null
    options.position = back?.position ?? null
  }
  return JSON.stringify(options)
}

let guarded = false
/** Route the engine's url sync through hostUrlOptions; a no-op until boot.js has defined it. */
export function guardUrlSync(pageDir: string): void {
  const w = window as HostWindow
  const sync = w.set_url_params
  if (guarded || sync == null) return
  guarded = true
  w.set_url_params = (optionsJson) => sync(hostUrlOptions(optionsJson, `${pageDir}preview/`, home))
}

/** The scene `/spawn` loads from a realm: the first its `/about` lists. */
function sceneHashFromAbout(about: unknown): string {
  const urn = (about as { configurations?: { scenesUrn?: unknown[] } } | null)?.configurations?.scenesUrn?.[0]
  const hash = typeof urn === 'string' ? /^urn:decentraland:entity:([^?]+)/.exec(urn)?.[1] : undefined
  if (hash == null) throw new Error('the editor scene realm lists no scene')
  return hash
}

let mounted = false
/** Publish the host object and load the editor package. Once per page. */
export function mountEditorHost(source: EditorSource, pageDir: string, deps: EditorHostDeps): void {
  if (mounted) return
  mounted = true
  const w = window as HostWindow
  const sceneUrl = `${source.base}scene`
  guardUrlSync(pageDir)
  bridgeChannelName() // seeds __bridgeSession when nothing has yet

  const container = document.createElement('div')
  container.id = 'dcl-editor-host'
  container.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:var(--z-editor)'
  document.body.appendChild(container)

  let scene: Promise<{ hash: string }> | null = null

  w.__dclEditorHost = {
    version: 0,
    pageDir,
    editorBase: source.base,
    busSession: w.__bridgeSession ?? '',
    container,
    engineConsole: deps.engineConsole,
    identity: deps.identity,
    setMode: deps.setMode,
    async openPreview(projectId, position) {
      const parcel = PARCEL.exec(position)
      if (!PROJECT_ID.test(projectId) || parcel == null) throw new Error('openPreview: invalid project id or position')
      const q = new URLSearchParams(location.search)
      home ??= { realm: q.get('realm'), position: q.get('position') }
      // no trailing slash: the engine appends /about
      await deps.travel(`${pageDir}preview/${projectId}`, { x: Number(parcel[1]), y: Number(parcel[2]) })
    },
    spawnEditorScene() {
      scene ??= (async () => {
        const about = await fetch(`${sceneUrl}/about`)
        if (!about.ok) throw new Error(`the editor scene realm answered ${about.status}`)
        const hash = sceneHashFromAbout(await about.json())
        await deps.scene('spawn', sceneUrl, hash)
        return { hash }
      })()
      scene.catch(() => (scene = null))
      return scene
    },
    exit() {
      try {
        w.__dclEditor?.unmount()
      } catch (e) {
        console.error('[editor host] unmount failed', e)
      }
      const spawned = scene
      scene = null
      void spawned
        ?.then(({ hash }) => deps.scene('kill', sceneUrl, hash))
        .catch((e: unknown) => console.error('[editor host] killing the editor scene failed', e))
      deps.setMode('off')
      const back = home
      home = null
      if (back == null) return
      const parcel = PARCEL.exec(back.position ?? '')
      deps
        .travel(back.realm ?? DEFAULT_REALM, parcel ? { x: Number(parcel[1]), y: Number(parcel[2]) } : undefined)
        .catch((e: unknown) => console.error('[editor host] travelling back failed', e))
    }
  }

  const script = document.createElement('script')
  script.type = 'module'
  script.src = `${source.base}editor.js`
  if (source.integrity != null) {
    script.integrity = source.integrity
    script.crossOrigin = 'anonymous'
  }
  document.head.appendChild(script)
}

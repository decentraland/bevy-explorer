// The page's side of the editor host contract (v1): the object the editor package finds at
// window.__dclEditorHost, the element it mounts into, and the script tag that loads it.
// This directory is its own script (editorHost.ts), added to the page when the editor is first
// opened: keep the HUD's modules out of its imports, types aside.

import type { AuthChainLink, AuthIdentity } from '../../auth/sso'
import type { EditorServices, EditorSource } from '../config'
import type { EditorHudMode } from '../hudMode'
import { signDeployment, signFetch } from './sign'

export interface DeploymentRequest {
  world: string
  entityId: string
  title: string
  fileCount: number
  bytes: number
}

export interface DclEditorHostV1 {
  version: 1
  /** Directory url of this page (the service worker's scope). */
  pageDir: string
  /** Directory url the editor package is served from. */
  editorBase: string
  /** The per-tab BroadcastChannel suffix; page-side code appends `#<busSession>` itself. */
  busSession: string
  /** Covers the viewport, click-through, above the canvas: the editor mounts inside it. */
  container: HTMLElement
  /** The project `?editor=<id>` asked for; null once the editor has been left. */
  openProject: string | null
  services: EditorServices
  engineConsole: (line: string) => Promise<string>
  identity: () => { address: string | null; isGuest: boolean }
  setMode: (mode: 'edit' | 'play' | 'off') => void
  /** Travel the running engine to `<pageDir>preview/<projectId>` at parcel `x,y`. */
  openPreview: (projectId: string, position: string) => Promise<void>
  /** Spawn the package's own scene (`<editorBase>scene`) with its permissions; resolves once live. */
  spawnEditorScene: () => Promise<{ hash: string }>
  /** fetch a url under `services.projects` as the signed-in wallet. Rejects 'not-allowed' for any
   *  other url and 'not-signed-in' for a guest. */
  signedFetch: (url: string, init?: { method?: string; headers?: Record<string, string>; body?: BodyInit | null }) => Promise<Response>
  /** Ask the player to confirm a Worlds deployment, then sign its entity id. Rejects 'cancelled'
   *  when declined and 'not-signed-in' for a guest. */
  signDeployment: (request: DeploymentRequest) => Promise<AuthChainLink[]>
  /** Unmount the editor, kill its scene, restore the HUD and the clock, and travel back. */
  exit: () => void
}

/** What the host needs from the HUD, read at call time. */
export interface EditorHostDeps {
  busSession: string
  /** Where exit() goes when the entry url named no realm. */
  defaultRealm: string
  engineConsole: (line: string) => Promise<string>
  identity: () => { address: string | null; isGuest: boolean }
  /** The stored identity of the wallet the player is in-world as; null for a guest. */
  login: () => AuthIdentity | null
  /** The player's answer to "sign this deployment as `wallet`?". */
  confirmDeployment: (request: DeploymentRequest, wallet: string) => Promise<boolean>
  setMode: (mode: EditorHudMode) => void
  travel: (realm: string, parcel?: { x: number; y: number }) => Promise<void>
  scene: (action: 'spawn' | 'kill', source: string, hash: string) => Promise<void>
}

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  __dclEditor?: { mount: () => void; unmount: () => void }
  set_url_params?: (optionsJson: string) => void
}

interface Home {
  realm: string | null
  position: string | null
}

// deploy/web/PREVIEW_REALM.md
const PROJECT_ID = /^[a-z0-9][a-z0-9-]{0,63}$/
const PARCEL = /^(-?\d+),(-?\d+)$/
// a bare content hash: nothing a signed fetch or a login message could be mistaken for
const ENTITY_ID = /^[A-Za-z0-9]{46,64}$/

// Where the player was before the first preview.
let home: Home | null = null
// The console line that restores the clock the player had when the editor was opened.
let clock: Promise<string | null> | null = null
let published = false
let attempts = 0

/** The engine's url-sync options as the page records them: `editor` as the entry url had it (the
 *  engine echoes its own, unset, flag) and a preview realm replaced by the place to go back to. */
function hostUrlOptions(optionsJson: string, previewRoot: string, back: Home | null, flag: string | boolean): string {
  const options = JSON.parse(optionsJson) as Record<string, unknown>
  options.editor = flag
  if (typeof options.realm === 'string' && options.realm.startsWith(previewRoot)) {
    options.realm = back?.realm ?? null
    options.position = back?.position ?? null
  }
  return JSON.stringify(options)
}

let guarded = false
/** Route the engine's url sync through hostUrlOptions; a no-op until boot.js has defined it.
 *  `flag` is the entry url's `editor` value (false when it had none). */
export function guardUrlSync(pageDir: string, flag: string | boolean): void {
  const w = window as HostWindow
  const sync = w.set_url_params
  if (guarded || sync == null) return
  guarded = true
  w.set_url_params = (optionsJson) => sync(hostUrlOptions(optionsJson, `${pageDir}preview/`, home, flag))
}

/** The scene `/spawn` loads from a realm: the first its `/about` lists. */
function sceneHashFromAbout(about: unknown): string {
  const urn = (about as { configurations?: { scenesUrn?: unknown[] } } | null)?.configurations?.scenesUrn?.[0]
  const hash = typeof urn === 'string' ? /^urn:decentraland:entity:([^?]+)/.exec(urn)?.[1] : undefined
  if (hash == null) throw new Error('the editor scene realm lists no scene')
  return hash
}

/** `url` normalised, when it is under `base`; null otherwise. */
function under(base: string | null, url: string): string | null {
  if (base == null || !URL.canParse(url)) return null
  const { href } = new URL(url)
  return href.startsWith(`${base}/`) ? href : null
}

/** The console line that puts the clock back, from the engine's reply to a bare `/time`. */
function clockRestore(reply: string): string | null {
  const clock = /speed (\S+) \(elapsed: ([\d.]+)\)/.exec(reply)
  return clock == null ? null : `/time ${Number(clock[2]) / 3600} ${clock[1]}`
}

/** Publish the host object and its container. */
function publishHost(source: EditorSource, pageDir: string, deps: EditorHostDeps, flag: string | boolean): void {
  const w = window as HostWindow
  const sceneUrl = `${source.base}scene`
  guardUrlSync(pageDir, flag)

  const container = document.createElement('div')
  container.id = 'dcl-editor-host'
  container.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:var(--z-editor)'
  document.body.appendChild(container)

  let scene: Promise<{ hash: string }> | null = null

  const host: DclEditorHostV1 = {
    version: 1,
    pageDir,
    editorBase: source.base,
    busSession: deps.busSession,
    container,
    openProject: typeof flag === 'string' ? flag : null,
    services: source.services,
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
    async signedFetch(url, init) {
      const target = under(source.services.projects, url)
      if (target == null) throw new Error('not-allowed')
      const identity = deps.login()
      if (identity == null) throw new Error('not-signed-in')
      const method = (init?.method ?? 'GET').toUpperCase()
      const own = Object.entries(init?.headers ?? {}).filter(([name]) => !/^x-identity-/i.test(name))
      const signed = await signFetch(identity, method, target)
      return fetch(target, { method, body: init?.body, headers: { ...Object.fromEntries(own), ...signed } })
    },
    async signDeployment(request) {
      const identity = deps.login()
      if (identity == null) throw new Error('not-signed-in')
      if (!ENTITY_ID.test(request.entityId)) throw new Error('signDeployment: invalid entity id')
      if (!(await deps.confirmDeployment(request, identity.authChain[0].payload))) throw new Error('cancelled')
      return signDeployment(identity, request.entityId)
    },
    exit() {
      try {
        w.__dclEditor?.unmount()
      } catch (e) {
        console.error('[editor host] unmount failed', e)
      }
      host.openProject = null
      const spawned = scene
      scene = null
      void spawned
        ?.then(({ hash }) => deps.scene('kill', sceneUrl, hash))
        .catch((e: unknown) => console.error('[editor host] killing the editor scene failed', e))
      // the editor pins the scene it inspects and stops the clock to edit
      const restore = clock
      clock = null
      void deps
        .engineConsole('/set_scene')
        .then(() => restore)
        .then((line) => (line == null ? undefined : deps.engineConsole(line)))
        .catch((e: unknown) => console.error('[editor host] restoring the engine failed', e))
      deps.setMode('off')
      const back = home
      home = null
      if (back == null) return
      const parcel = PARCEL.exec(back.position ?? '')
      deps
        .travel(back.realm ?? deps.defaultRealm, parcel ? { x: Number(parcel[1]), y: Number(parcel[2]) } : undefined)
        .catch((e: unknown) => console.error('[editor host] travelling back failed', e))
    }
  }
  w.__dclEditorHost = host
}

/** Open the editor: the first time publishes the host and loads the package, later times mount
 *  it again. Settles when the editor has taken over, or rejects when its package failed to load.
 *  `flag` is the entry url's `editor` value (false when it had none). */
export function openEditor(source: EditorSource, pageDir: string, deps: EditorHostDeps, flag: string | boolean): Promise<void> {
  const w = window as HostWindow
  clock ??= deps.engineConsole('/time').then(clockRestore, () => null)
  if (!published) {
    published = true
    publishHost(source, pageDir, deps, flag)
  }
  if (w.__dclEditor != null) {
    w.__dclEditor.mount()
    return Promise.resolve()
  }
  return new Promise((resolve, reject) => {
    const script = document.createElement('script')
    script.type = 'module'
    // a module that failed to load stays failed under its url
    script.src = `${source.base}editor.js${attempts++ === 0 ? '' : `?retry=${attempts}`}`
    if (source.integrity != null) {
      script.integrity = source.integrity
      script.crossOrigin = 'anonymous'
    }
    const failed = (): void => {
      script.remove()
      clock = null
      deps.setMode('off')
      reject(new Error('the editor package failed to load'))
    }
    script.onload = () => (w.__dclEditor != null ? resolve() : failed())
    script.onerror = failed
    document.head.appendChild(script)
  })
}

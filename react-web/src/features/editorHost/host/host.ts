// The page's side of the editor host contract (v1.2): the object the editor package finds at
// window.__dclEditorHost, the element it mounts into, and the script tag that loads it.

import type { AuthChainLink, AuthIdentity } from '../../auth/sso'
import type { EditorHudMode } from '../hudMode'
import { PROJECT_ID, type EditorServices, type EditorSource, type SignedServices } from '../source'
import { CID_PATTERN } from './cid'
import { sceneIdFromAbout, stageEditorScene } from './editorScene'
import type { Signer } from './signer'
import type { SignMessage } from './sign'
import { inShell, shellRequest } from '../../../lib/shell'
// Vite's worker pipeline is what bundles a file on its own; this one is loaded by script tag.
import signerUrl from './signer.ts?worker&url'

export interface DeploymentRequest {
  world: string
  entityId: string
  title: string
  fileCount: number
  bytes: number
}

export interface UndeployRequest {
  world: string
  /** null: the whole world, every scene in it */
  coordinate: string | null
}

export interface SignedFetchInit {
  method?: string
  headers?: Record<string, string>
  body?: BodyInit | null
  /** The x-identity-metadata of a request to `services.signed`; ignored for `services.projects`. */
  metadata?: Record<string, unknown>
  signal?: AbortSignal
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
  /** Spawn the package's own scene (`<editorBase>scene`, checked against the pin and served by
   *  the page) with its permissions; resolves once live. */
  spawnEditorScene: () => Promise<{ hash: string }>
  /** fetch a url under `services.projects` or `services.signed` as the signed-in wallet. Rejects
   *  'not-allowed' for any other url, method or editor metadata, 'cancelled' when the player declines
   *  an undeploy, and 'not-signed-in' for a guest. */
  signedFetch: (url: string, init?: SignedFetchInit) => Promise<Response>
  /** fetch `path` (`/values…`, `/players/<address>/values…` or `/env/<key>`) from the previewed
   *  project's storage. Rejects 'not-allowed' for any other path or with no preview open. */
  previewStorageFetch: (path: string, init?: { method?: string; headers?: Record<string, string>; body?: string }) => Promise<Response>
  /** the open preview's scene server console, the lines after `after` (a `seq` it
   *  returned, 0 for all it keeps); empty with no server running. */
  previewServerLogs: (after: number) => ServerLogLine[]
  /** the open preview has new content (a build landed); its scene server restarts on it. */
  previewChanged: () => void
  /** Ask the player to confirm a Worlds deployment, then sign its entity id. Rejects 'cancelled'
   *  when declined and 'not-signed-in' for a guest. */
  signDeployment: (request: DeploymentRequest) => Promise<AuthChainLink[]>
  /** Leave the scene as exit() does, then show the HUD's Create page. */
  openCreatePage: () => void
  /** Unmount the editor, kill its scene, restore the HUD and the clock, and travel back. */
  exit: () => void
}

export interface ServerLogLine {
  seq: number
  level: 'log' | 'error' | 'system'
  msg: string
}

/** What editor.js leaves at window.__dclEditor. */
export interface EditorPackage {
  /** Render the editor's home into `el` (the Create page's body); returns its unmount. */
  mountHome: (el: HTMLElement, api: { close: () => void }) => () => void
  unmount: () => void
}

/** What the host needs from the HUD, read at call time. */
export interface EditorHostDeps {
  busSession: string
  /** Where exit() goes when the entry url named no realm. */
  defaultRealm: string
  engineConsole: (line: string) => Promise<string>
  identity: () => { address: string | null; isGuest: boolean }
  /** The stored identity of the wallet the player is in-world as; null for a guest. */
  login: () => Promise<AuthIdentity | null>
  /** The player's answer to "sign this deployment as `wallet`, for `server`?". */
  confirmDeployment: (request: DeploymentRequest, wallet: string, server: string) => Promise<boolean>
  /** The player's answer to "sign removing this scene from `request.world` as `wallet`?". */
  confirmUndeploy: (request: UndeployRequest, wallet: string, server: string, signal?: AbortSignal) => Promise<boolean>
  setMode: (mode: EditorHudMode) => void
  showCreatePage: (open: boolean) => void
  travel: (realm: string, parcel?: { x: number; y: number }) => Promise<void>
  scene: (action: 'spawn' | 'kill', source: string, hash: string) => Promise<void>
}

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  __dclEditor?: Partial<EditorPackage>
  set_url_params?: (optionsJson: string) => void
  __dclEditorSigner?: (signer: Signer) => void
  __bevyStartServer?: (options: Record<string, unknown> & { realm: string; position: string; preview: true }) => Promise<unknown>
  __bevyBootConfig?: Record<string, unknown>
}

interface Home {
  realm: string | null
  position: string | null
}

const PARCEL = /^(-?\d+),(-?\d+)$/
// engine.js runs each scene server in a hidden frame and has no stop: removing the frame ends it
const SERVER_FRAME = 'iframe[src$="/headless.html"]'
// What the editor drives through the console: the scene it edits. Never spawn, kill, login or a
// realm change, which would go around spawnEditorScene and openPreview.
const EDITOR_COMMANDS = new Set([
  'component_default', 'component_names', 'component_schema', 'crdt_initial', 'crdt_snapshot', 'debug_colliders',
  'delete_component', 'delete_entity', 'freeze_scene', 'highlight', 'move_player_to', 'new_entity', 'player_position',
  'reload', 'save_composite', 'scene_content', 'scene_logs', 'scene_stats', 'screenshot', 'set_component',
  'set_component_raw', 'set_scene', 'texture_camera_screenshot', 'tick_scene', 'time', 'unfreeze_scene'
])
// headers the editor may send on a request signed as the player
const SIGNED_FETCH_HEADERS = new Set(['accept', 'content-type', 'if-match', 'if-none-match', 'x-confirm-delete-all'])
// what the editor may ask of each service a scene's signed fetch reaches
const SIGNED_METHODS: Record<keyof SignedServices, ReadonlySet<string>> = {
  worldsContent: new Set(['GET', 'PUT', 'DELETE']),
  commsGatekeeper: new Set(['GET', 'POST', 'PUT', 'DELETE']),
  storage: new Set(['GET', 'POST', 'PUT', 'DELETE']),
  creatorsData: new Set(['POST']),
  multiplayer: new Set(['GET'])
}
// the server's router ignores case and a trailing slash
const UNDEPLOY = /^\/world\/([^/]+)\/scenes\/([^/]+)\/?$/i
const UNDEPLOY_WORLD = /^\/entities\/([^/]+)\/?$/i
// a preview realm's storage routes (deploy/web/PREVIEW_REALM.md "Storage")
const PREVIEW_STORAGE_PATH = /^(?:values|players\/[^/]+\/values)(?:\/[^/]+)?$|^env\/[^/]+$/

// Where the player was before the first preview.
let home: Home | null = null
// The home being travelled back to: the url still names the preview realm until the travel lands.
let returning: Home | null = null
// The console line that restores the clock the player had when the editor was opened.
let clock: Promise<string | null> | null = null
let published = false
let editorAttempts = 0
let editor: Promise<EditorPackage> | null = null

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

// a module that failed to load stays failed under its url, so a retry loads it under another
function attemptUrl(url: string, attempt: number): string {
  return attempt === 1 ? url : `${url}${url.includes('?') ? '&' : '?'}retry=${attempt}`
}

// inside the web shell the key stays in the shell, which signs on request (src/shell/main.ts)
function signerFor(loaded: Signer, identity: AuthIdentity): SignMessage {
  return inShell ? (address, message) => shellRequest<string>('sign', { signer: address, message }) : loaded.localSigner(identity)
}

let signer: Promise<Signer> | null = null
let signerAttempts = 0
/** The signing script, added to the page the first time it is needed. */
function loadSigner(): Promise<Signer> {
  signer ??= new Promise<Signer>((resolve, reject) => {
    const w = window as HostWindow
    const script = document.createElement('script')
    script.type = 'module'
    script.src = attemptUrl(signerUrl, ++signerAttempts)
    w.__dclEditorSigner = (loaded) => {
      delete w.__dclEditorSigner
      resolve(loaded)
    }
    script.onerror = () => {
      delete w.__dclEditorSigner
      script.remove()
      signer = null
      reject(new Error('the signer failed to load'))
    }
    document.head.appendChild(script)
  })
  return signer
}

let guarded = false
/** Route the engine's url sync through hostUrlOptions; a no-op until boot.js has defined it.
 *  `flag` is the entry url's `editor` value (false when it had none). */
export function guardUrlSync(pageDir: string, flag: string | boolean): void {
  const w = window as HostWindow
  const sync = w.set_url_params
  if (guarded || sync == null) return
  guarded = true
  w.set_url_params = (optionsJson) => sync(hostUrlOptions(optionsJson, `${pageDir}preview/`, home ?? returning, flag))
}

/** Whether the preview realm's scene runs its own server (scene.json `authoritativeMultiplayer`). */
async function authoritative(realm: string): Promise<boolean> {
  const res = await fetch(`${realm}/scene.json`)
  if (!res.ok) return false
  const scene: unknown = await res.json()
  return typeof scene === 'object' && scene != null && 'authoritativeMultiplayer' in scene && scene.authoritativeMultiplayer === true
}

/** `url` normalised, when it is under `base`; null otherwise. */
function under(base: string | null, url: string): string | null {
  if (base == null || !URL.canParse(url)) return null
  const { href } = new URL(url)
  return href.startsWith(`${base}/`) ? href : null
}

/** Which of `services` `url` is under, and its normalised form; null for none or a request the
 *  editor may not sign there. */
function signedTarget(services: SignedServices | undefined, url: string, method: string): { service: keyof SignedServices; target: string } | null {
  if (services == null) return null
  for (const service of Object.keys(SIGNED_METHODS) as Array<keyof SignedServices>) {
    const target = under(services[service], url)
    if (target == null) continue
    if (!SIGNED_METHODS[service].has(method)) return null
    if (service === 'multiplayer' && new URL(target).pathname !== '/logs') return null
    return { service, target }
  }
  return null
}

// a signature only the editor's own service should get; checked as signed, lowercased
function editorMetadata(metadata: Record<string, unknown>): boolean {
  const folded = JSON.parse(JSON.stringify(metadata).toLowerCase()) as Record<string, unknown>
  return folded.signer === 'dcl:editor' || (typeof folded.intent === 'string' && folded.intent.startsWith('dcl:editor:'))
}

/** The console line that puts the clock back, from the engine's reply to a bare `/time`. */
function clockRestore(reply: string): string | null {
  const clock = /speed (\S+) \(elapsed: ([\d.]+)\)/.exec(reply)
  return clock == null ? null : `/time ${Number(clock[2]) / 3600} ${clock[1]}`
}

function publishHost(source: EditorSource, pageDir: string, deps: EditorHostDeps, flag: string | boolean): void {
  const w = window as HostWindow
  const packageScene = `${source.base}scene`
  guardUrlSync(pageDir, flag)

  const container = document.createElement('div')
  container.id = 'dcl-editor-host'
  container.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:var(--z-editor)'
  document.body.appendChild(container)

  let scene: Promise<{ hash: string; realm: string }> | null = null
  let editorHash: string | null = null
  let previewing: string | null = null
  // the project being previewed (its `b64-` entity id names /preview/<id>-<machine>) or the editor's own scene
  const editedScene = (id: string | undefined): boolean => {
    if (id == null) return false
    if (id === editorHash) return true
    if (previewing == null || !id.startsWith('b64-')) return false
    try {
      const path = atob(id.slice(4))
      return path === `/preview/${previewing}` || path.startsWith(`/preview/${previewing}-`)
    } catch {
      return false
    }
  }
  // before the editor first stops the clock to edit
  const readClock = (): void => {
    clock ??= deps.engineConsole('/time').then(clockRestore, () => null)
  }

  // one switch at a time, so a stop never lands between another preview's start and its frame
  let serverSwitch: Promise<void> = Promise.resolve()
  let served: { realm: string; position: string } | null = null
  let restartQueued = false
  // the host's count of server log lines (previewServerLogs): where the running frame's start, and the last given out
  let logBase = 0
  let logLatest = 0
  const serveScene = (realm: string | null, position = ''): Promise<void> => {
    served = realm == null ? null : { realm, position }
    serverSwitch = serverSwitch.then(async () => {
      for (const frame of document.querySelectorAll(SERVER_FRAME)) frame.remove()
      logBase = logLatest
      if (realm == null) return
      try {
        if (!(await authoritative(realm))) return
        if (w.__bevyStartServer == null) throw new Error('this engine cannot run a scene server')
        // the client's backends (base domain, service overrides) too, or the two never meet; the
        // server reads only the shared launch options
        await w.__bevyStartServer({ ...w.__bevyBootConfig, realm, position, preview: true })
      } catch (e) {
        console.error('[editor host] starting the scene server failed', e)
      }
    })
    return serverSwitch
  }

  const leave = (): void => {
    void serveScene(null)
    try {
      w.__dclEditor?.unmount?.()
    } catch (e) {
      console.error('[editor host] unmount failed', e)
    }
    host.openProject = null
    previewing = null
    editorHash = null
    const spawned = scene
    scene = null
    void spawned
      ?.then(({ hash, realm }) => deps.scene('kill', realm, hash))
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
    returning = back
    const parcel = PARCEL.exec(back.position ?? '')
    deps
      .travel(back.realm ?? deps.defaultRealm, parcel ? { x: Number(parcel[1]), y: Number(parcel[2]) } : undefined)
      .catch((e: unknown) => console.error('[editor host] travelling back failed', e))
      .finally(() => {
        if (returning === back) returning = null
      })
  }

  const host: DclEditorHostV1 = {
    version: 1,
    pageDir,
    editorBase: source.base,
    busSession: deps.busSession,
    container,
    openProject: typeof flag === 'string' ? flag : null,
    services: source.services,
    engineConsole(line) {
      const [name = '', ...args] = line.trim().replace(/^\//, '').split(/\s+/)
      const namesScene = name === 'reload' || (name === 'set_scene' && args.length > 0)
      if (!EDITOR_COMMANDS.has(name) || (namesScene && !editedScene(args.at(0)))) return Promise.reject(new Error('not-allowed'))
      return deps.engineConsole(line)
    },
    identity: deps.identity,
    setMode(mode) {
      if (mode !== 'off') readClock()
      deps.setMode(mode)
    },
    async openPreview(projectId, position) {
      const parcel = PARCEL.exec(position)
      if (!PROJECT_ID.test(projectId) || parcel == null) throw new Error('openPreview: invalid project id or position')
      readClock()
      const q = new URLSearchParams(location.search)
      home ??= returning ?? { realm: q.get('realm'), position: q.get('position') }
      previewing = projectId
      // no trailing slash: the engine appends /about
      const realm = `${pageDir}preview/${projectId}`
      // before the trip, so the client finds its server there
      await serveScene(realm, position)
      await deps.travel(realm, { x: Number(parcel[1]), y: Number(parcel[2]) })
    },
    spawnEditorScene() {
      scene ??= (async () => {
        let hash = source.editorSceneEntity
        if (hash == null) {
          // a dev package: the scene its realm lists
          const about = await fetch(`${packageScene}/about`)
          if (!about.ok) throw new Error(`the editor scene realm answered ${about.status}`)
          hash = sceneIdFromAbout(await about.json())
        }
        const realm = await stageEditorScene(packageScene, hash, pageDir)
        await deps.scene('spawn', realm, hash)
        editorHash = hash
        return { hash, realm }
      })()
      const pending = scene
      // leave() may already have started another
      pending.catch(() => {
        if (scene === pending) scene = null
      })
      return pending.then(({ hash }) => ({ hash }))
    },
    async signedFetch(url, init) {
      const method = (init?.method ?? 'GET').toUpperCase()
      const project = under(source.services.projects, url)
      const other = project == null ? signedTarget(source.services.signed, url, method) : null
      const target = project ?? other?.target
      if (target == null) throw new Error('not-allowed')
      const metadata = other == null ? undefined : (init?.metadata ?? {})
      if (metadata != null && (typeof metadata !== 'object' || Array.isArray(metadata) || editorMetadata(metadata))) throw new Error('not-allowed')
      const identity = await deps.login()
      if (identity == null) throw new Error('not-signed-in')
      const signal = init?.signal
      signal?.throwIfAborted()
      const path = new URL(target).pathname
      const undeploy = other?.service === 'worldsContent' && method === 'DELETE' ? (UNDEPLOY.exec(path) ?? UNDEPLOY_WORLD.exec(path)) : null
      if (undeploy != null) {
        const request = { world: decodeURIComponent(undeploy[1]), coordinate: undeploy[2] == null ? null : decodeURIComponent(undeploy[2]) }
        const confirmed = await deps.confirmUndeploy(request, identity.authChain[0].payload, new URL(target).host, signal)
        signal?.throwIfAborted()
        if (!confirmed) throw new Error('cancelled')
      }
      const own = Object.entries(init?.headers ?? {}).filter(([name]) => SIGNED_FETCH_HEADERS.has(name.toLowerCase()))
      const loaded = await loadSigner()
      const signed = await loaded.signFetch(identity, method, target, signerFor(loaded, identity), metadata)
      return fetch(target, { method, body: init?.body, signal, headers: { ...Object.fromEntries(own), ...signed } })
    },
    previewStorageFetch(path, init) {
      if (previewing == null) return Promise.reject(new Error('not-allowed'))
      const base = new URL(`${pageDir}preview/${previewing}/`)
      const target = new URL(path.replace(/^\/+/, ''), base)
      const route = target.href.startsWith(base.href) ? target.pathname.slice(base.pathname.length) : ''
      if (!PREVIEW_STORAGE_PATH.test(route)) return Promise.reject(new Error('not-allowed'))
      return fetch(target.href, { method: init?.method ?? 'GET', headers: init?.headers, body: init?.body })
    },
    previewChanged() {
      // builds land in bursts: one restart waits behind the switch in progress, with the latest content
      if (served == null || restartQueued) return
      restartQueued = true
      void serverSwitch.then(() => {
        restartQueued = false
        if (served != null) void serveScene(served.realm, served.position)
      })
    },
    previewServerLogs(after) {
      const frame = document.querySelector<HTMLIFrameElement>(SERVER_FRAME)
      const read = (frame?.contentWindow as (Window & { sceneLogsAfter?: (after: number) => ServerLogLine[] }) | null | undefined)?.sceneLogsAfter
      if (read == null || !Number.isFinite(after)) return []
      // each server frame counts from 0; on the host's count a restarted server's lines come after
      // the last one's, so a cursor never skips them
      const lines = read(Math.max(0, after - logBase)).map((line) => ({ ...line, seq: logBase + line.seq }))
      if (lines.length > 0) logLatest = Math.max(logLatest, lines[lines.length - 1].seq)
      return lines.filter((line) => line.seq > after)
    },
    async signDeployment(request) {
      const identity = await deps.login()
      if (identity == null) throw new Error('not-signed-in')
      // a bare content hash: nothing a signed fetch or a login message could be mistaken for
      if (!CID_PATTERN.test(request.entityId)) throw new Error('signDeployment: invalid entity id')
      const server = new URL(source.services.worldsContent).host
      if (!(await deps.confirmDeployment(request, identity.authChain[0].payload, server))) throw new Error('cancelled')
      const loaded = await loadSigner()
      return loaded.signDeployment(identity, request.entityId, signerFor(loaded, identity))
    },
    openCreatePage() {
      leave()
      deps.showCreatePage(true)
    },
    exit() {
      leave()
      deps.showCreatePage(false)
    }
  }
  w.__dclEditorHost = host
}

/** Publish the host (once) and load the editor package (once). Rejects when the package failed
 *  to load or has no mountHome; a later call tries again. `flag` is the entry url's `editor`
 *  value (false when it had none). */
export function loadEditor(source: EditorSource, pageDir: string, deps: EditorHostDeps, flag: string | boolean): Promise<EditorPackage> {
  const w = window as HostWindow
  if (!published) {
    published = true
    publishHost(source, pageDir, deps, flag)
  }
  editor ??= new Promise<EditorPackage>((resolve, reject) => {
    const script = document.createElement('script')
    const failed = (): void => {
      script.remove()
      editor = null
      reject(new Error('the editor package failed to load'))
    }
    const loaded = (): void => {
      const pkg = w.__dclEditor
      if (typeof pkg?.mountHome === 'function' && typeof pkg.unmount === 'function') return resolve(pkg as EditorPackage)
      try {
        // an older package mounts itself on load
        pkg?.unmount?.()
      } catch (e) {
        console.error('[editor host] unmount failed', e)
      }
      failed()
    }
    if (w.__dclEditor != null) return loaded()
    script.type = 'module'
    script.src = attemptUrl(`${source.base}editor.js`, ++editorAttempts)
    if (source.editorJsIntegrity != null) {
      script.integrity = source.editorJsIntegrity
      script.crossOrigin = 'anonymous'
    }
    script.onload = loaded
    script.onerror = failed
    document.head.appendChild(script)
  })
  return editor
}

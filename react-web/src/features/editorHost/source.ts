// Where the editor package comes from and which services it talks to (README "Releasing the
// editor"). Read by the Create page's chunk only; config.ts holds what the HUD itself needs.

import { isLoopback, RELEASED_EDITOR } from './config'

/** An editor package the page runs from one directory. */
export interface EditorPin {
  /** The package's versioned directory url, with the trailing slash. */
  base: string
  /** SRI hash of `<base>editor.js` (it covers the editor's own manifest); null: not checked. */
  editorJsIntegrity: string | null
  /** Entity id of the package's editor scene, `<base>scene/<id>`; null: `scene/about`'s. */
  editorSceneEntity: string | null
}

/** A published package followed by npm dist-tag: the tag resolves to a version each time the
 *  editor loads, so a release is a publish on that tag and no explorer deploy. */
export interface EditorRelease {
  package: string
  tag: string
}

const REGISTRY = 'https://registry.npmjs.org'
const CDN = 'https://cdn.jsdelivr.net/npm'
// the registry's answer builds a url: only a version gets in
const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/

/** The pin a release resolves to now. The CDN caches a tag for days, so the registry (never
 *  cached) names the version and the CDN serves that immutable directory. */
export async function releasedPin(release: EditorRelease): Promise<EditorPin> {
  const res = await fetch(`${REGISTRY}/${release.package}/${release.tag}`, { cache: 'no-cache' })
  if (!res.ok) throw new Error(`the npm registry answered ${res.status} for ${release.package}@${release.tag}`)
  const { version } = (await res.json()) as { version?: unknown }
  if (typeof version !== 'string' || !VERSION.test(version)) throw new Error(`${release.package}@${release.tag} names no version`)
  return { base: `${CDN}/${release.package}@${version}/`, editorJsIntegrity: null, editorSceneEntity: null }
}

export interface EditorServices {
  /** Base url (no trailing slash) of the project storage service; null = none, sync is off. */
  projects: string | null
  /** Base url of the Worlds content server deployments go to. */
  worldsContent: string
  /** The services the editor may sign requests to as a scene would; only beside a real Worlds server. */
  signed?: SignedServices
}

/** Base urls (no trailing slash) host.signedFetch signs for, pathname only. */
export interface SignedServices {
  worldsContent: string
  commsGatekeeper: string
  storage: string
  creatorsData: string
  multiplayer: string
}

export interface EditorSource extends EditorPin {
  services: EditorServices
}

const signedFor = (tld: 'zone' | 'org'): SignedServices => ({
  worldsContent: `https://worlds-content-server.decentraland.${tld}`,
  commsGatekeeper: `https://comms-gatekeeper.decentraland.${tld}`,
  storage: `https://storage.decentraland.${tld}`,
  creatorsData: 'https://creators-data.decentraland.org/v2',
  multiplayer: `https://multiplayer-server.decentraland.${tld}`
})
const SIGNED_SERVICES = [signedFor('zone'), signedFor('org')]

// a Worlds server of either environment brings its own; a local one none
function withSigned(services: Omit<EditorServices, 'signed'>): EditorServices {
  const signed = SIGNED_SERVICES.find((s) => s.worldsContent === services.worldsContent.replace(/\/+$/, ''))
  return signed == null ? services : { ...services, signed }
}

/** What the released package talks to: the project service (dcl-editor packages/service) and the
 *  session's Worlds server. The base domain, not ?worldsServer=, so a link can't redirect deployments. */
const pinnedServices = (baseDomain: string): EditorServices =>
  withSigned({
    projects: 'https://web-editor-dev.dclregenesislabs.xyz',
    worldsContent: `https://worlds-content-server.${baseDomain}`
  })

const LOOPBACK_SERVICES: EditorServices = {
  projects: 'http://localhost:8787',
  worldsContent: 'https://worlds-content-server.decentraland.org'
}
/** Entry params that point a loopback page's editor at other services (local ones, in the gates). */
const SERVICE_PARAMS = { projects: 'editor-projects', worldsContent: 'editor-worlds' } as const

// deploy/web/PREVIEW_REALM.md
export const PROJECT_ID = /^[a-z0-9][a-z0-9-]{0,63}$/

// local services only: a link must not point a player's projects or deployments at another server
function serviceUrl(raw: string | null): string | null {
  if (raw == null || !URL.canParse(raw)) return null
  const url = new URL(raw)
  return /^https?:$/.test(url.protocol) && isLoopback(url.hostname) ? (url.origin + url.pathname).replace(/\/+$/, '') : null
}

const ownPackage = (hostname: string): boolean => isLoopback(hostname) && (import.meta.env.DEV || RELEASED_EDITOR == null)

/** Whether this page has an editor to open: what `editorSource` resolves, known without asking. */
export const hasEditor = (hostname: string): boolean => ownPackage(hostname) || RELEASED_EDITOR != null

/** The editor this page opens, or null when it has none. A dev server on loopback serves its
 *  own package (vite.config.ts `editorPackage`); a production build follows the release wherever
 *  it runs. */
export async function editorSource(search: string, hostname: string, pageDir: string, baseDomain: string): Promise<EditorSource | null> {
  const loopback = isLoopback(hostname)
  const pin: EditorPin | null = ownPackage(hostname)
    ? { base: `${pageDir}editor/`, editorJsIntegrity: null, editorSceneEntity: null }
    : RELEASED_EDITOR == null
      ? null
      : await releasedPin(RELEASED_EDITOR)
  if (pin == null) return null
  if (!loopback) return { ...pin, services: pinnedServices(baseDomain) }
  const q = new URLSearchParams(search)
  return {
    ...pin,
    services: withSigned({
      projects: serviceUrl(q.get(SERVICE_PARAMS.projects)) ?? LOOPBACK_SERVICES.projects,
      worldsContent: serviceUrl(q.get(SERVICE_PARAMS.worldsContent)) ?? LOOPBACK_SERVICES.worldsContent
    })
  }
}

/** `?editor` opens the editor once in-world; `?editor=<projectId>` also names the project to
 *  open. The engine's url sync rewrites a bare flag as `editor=true`, so that is no project. */
export function editorEntry(search: string): { open: boolean; project: string | null } {
  const value = new URLSearchParams(search).get('editor')
  return { open: value != null, project: value != null && value !== 'true' && PROJECT_ID.test(value) ? value : null }
}

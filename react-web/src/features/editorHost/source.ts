// Where the editor package comes from and which services it talks to (README "Releasing the
// editor"). Read by the Create page's chunk only; config.ts holds what the HUD itself needs.

import { isLoopback, PINNED_EDITOR } from './config'

/** A released editor package: everything the page runs of it is checked against this. */
export interface EditorPin {
  /** The package's versioned directory url, with the trailing slash. */
  base: string
  /** SRI hash of `<base>editor.js` (it covers the editor's own manifest); null for a dev package. */
  editorJsIntegrity: string | null
  /** Entity id of the package's editor scene, `<base>scene/<id>`; null: `scene/about`'s (dev). */
  editorSceneEntity: string | null
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

/** What the released package talks to. projects: null until the storage service is deployed. */
const PINNED_SERVICES: EditorServices = withSigned({ projects: null, worldsContent: 'https://worlds-content-server.decentraland.zone' })

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

/** The editor this page opens, or null when it has none. A dev server on loopback serves its
 *  own package (vite.config.ts `editorPackage`); a production build uses the pin wherever it runs. */
export function editorSource(search: string, hostname: string, pageDir: string): EditorSource | null {
  const loopback = isLoopback(hostname)
  const dev: EditorPin = { base: `${pageDir}editor/`, editorJsIntegrity: null, editorSceneEntity: null }
  const pin = loopback && (import.meta.env.DEV || PINNED_EDITOR == null) ? dev : PINNED_EDITOR
  if (pin == null) return null
  if (!loopback) return { ...pin, services: PINNED_SERVICES }
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

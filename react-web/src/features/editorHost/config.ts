// Whether this page can host the scene editor, where the editor package comes from and which
// services it talks to. The editor is an external package: being available downloads nothing,
// it loads when the player opens Create or the entry url carries `editor`.

/** Deployments that may load the editor. Loopback hosts always may. */
const ALLOWED_HOSTS = ['decentraland.zone']
const LOOPBACK_HOSTS = ['localhost', '127.0.0.1']

interface EditorPackage {
  /** Directory url the package is served from. */
  base: string
  /** SRI hash of `<base>editor.js`; null when the package is same-origin (dev). */
  integrity: string | null
}

export interface EditorServices {
  /** Base url (no trailing slash) of the project storage service; null = none, sync is off. */
  projects: string | null
  /** Base url of the Worlds content server deployments go to. */
  worldsContent: string
}

export interface EditorSource extends EditorPackage {
  services: EditorServices
}

/** The released editor package: its versioned CDN directory (with the trailing slash) and the
 *  SRI hash of its `editor.js`. null = none released yet, so only loopback hosts get an editor. */
const PINNED_EDITOR = null as EditorPackage | null
/** What the released package talks to. projects: null until the storage service is deployed. */
const PINNED_SERVICES: EditorServices = { projects: null, worldsContent: 'https://worlds-content-server.decentraland.zone' }

const LOOPBACK_SERVICES: EditorServices = {
  projects: 'http://localhost:8787',
  worldsContent: 'https://worlds-content-server.decentraland.org'
}
/** Entry params that point a loopback page's editor at other services (local ones, in the gates). */
const SERVICE_PARAMS = { projects: 'editor-projects', worldsContent: 'editor-worlds' } as const

// deploy/web/PREVIEW_REALM.md
const PROJECT_ID = /^[a-z0-9][a-z0-9-]{0,63}$/

function serviceUrl(raw: string | null): string | null {
  if (raw == null || !URL.canParse(raw)) return null
  const url = new URL(raw)
  return /^https?:$/.test(url.protocol) ? (url.origin + url.pathname).replace(/\/+$/, '') : null
}

/** The editor this page may open, or null when it has none. */
export function editorSource(search: string, hostname: string, pageDir: string): EditorSource | null {
  if (LOOPBACK_HOSTS.includes(hostname)) {
    const q = new URLSearchParams(search)
    return {
      base: `${pageDir}editor/`,
      integrity: null,
      services: {
        projects: serviceUrl(q.get(SERVICE_PARAMS.projects)) ?? LOOPBACK_SERVICES.projects,
        worldsContent: serviceUrl(q.get(SERVICE_PARAMS.worldsContent)) ?? LOOPBACK_SERVICES.worldsContent
      }
    }
  }
  return ALLOWED_HOSTS.includes(hostname) && PINNED_EDITOR != null ? { ...PINNED_EDITOR, services: PINNED_SERVICES } : null
}

/** The entry params the editor host reads on this hostname. */
export function editorParams(hostname: string): string[] {
  return LOOPBACK_HOSTS.includes(hostname) ? ['editor', ...Object.values(SERVICE_PARAMS)] : ['editor']
}

/** `?editor` opens the editor once in-world; `?editor=<projectId>` also names the project to
 *  open. The engine's url sync rewrites a bare flag as `editor=true`, so that is no project. */
export function editorEntry(search: string): { open: boolean; project: string | null } {
  const value = new URLSearchParams(search).get('editor')
  return { open: value != null, project: value != null && value !== 'true' && PROJECT_ID.test(value) ? value : null }
}

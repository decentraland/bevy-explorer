// Whether this page hosts the scene editor, and where the editor package comes from. The editor
// is an external package: without the `editor` entry param on an allowed host, nothing loads.

/** Deployments that may load the editor. Loopback hosts always may. */
const ALLOWED_HOSTS = ['decentraland.zone']
const LOOPBACK_HOSTS = ['localhost', '127.0.0.1']

/** The released editor package: its versioned CDN directory (with the trailing slash) and the
 *  SRI hash of its `editor.js`. null = none released yet, so only loopback hosts get an editor. */
const PINNED_EDITOR: EditorSource | null = null

export interface EditorSource {
  /** Directory url the package is served from. */
  base: string
  /** SRI hash of `<base>editor.js`; null when the package is same-origin (dev). */
  integrity: string | null
}

/** The editor package this page may load, or null when it must not load one. */
export function editorSource(search: string, hostname: string, pageDir: string): EditorSource | null {
  if (!new URLSearchParams(search).has('editor')) return null
  if (LOOPBACK_HOSTS.includes(hostname)) return { base: `${pageDir}editor/`, integrity: null }
  return ALLOWED_HOSTS.includes(hostname) ? PINNED_EDITOR : null
}

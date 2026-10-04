// The in-tab scene server's storage (deploy/web/PREVIEW_REALM.md "Storage"): the service worker
// serves a preview realm's storage routes only to requests carrying the token kept here, which
// the engine adds to that realm's server copy alone.

// service_worker.js reads it; scenes cannot (sandbox_worker.js deletes `caches`)
const CACHE = 'dcl-editor-storage-v1'

// the header service_worker.js checks the token in, as the engine sends it
export const STORAGE_HEADER = 'x-dcl-local-server'

// per realm: every tab on the origin shares the cache
const accessKey = (realm: string): string => `${realm}/__server`

/** Open `realm`'s storage to its server copy. Null without Cache Storage. */
export async function grantStorage(realm: string): Promise<string | null> {
  if (typeof caches === 'undefined') return null
  const token = crypto.randomUUID()
  await (await caches.open(CACHE)).put(accessKey(realm), new Response(JSON.stringify({ token })))
  return token
}

/** Close `realm`'s storage if `token` still opens it; what is stored stays. */
export async function revokeStorage(realm: string, token: string): Promise<void> {
  if (typeof caches === 'undefined') return
  const cache = await caches.open(CACHE)
  const stored = await cache.match(accessKey(realm))
  const access: unknown = stored ? await stored.json().catch(() => null) : null
  if (access != null && typeof access === 'object' && 'token' in access && access.token === token) await cache.delete(accessKey(realm))
}

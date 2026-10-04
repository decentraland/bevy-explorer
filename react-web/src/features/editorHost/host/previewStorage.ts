// The in-tab scene server's storage (deploy/web/PREVIEW_REALM.md "Storage"): the service worker
// serves a preview realm's storage routes only to requests carrying the token kept here, which
// the engine adds to that realm's server copy alone.

// service_worker.js reads it; scenes cannot (sandbox_worker.js deletes `caches`)
const CACHE = 'dcl-editor-storage-v1'

const accessKey = (pageDir: string): string => `${pageDir}preview/__server`

/** Open `realm`'s storage to its server copy, and close every other's. Null without Cache Storage. */
export async function grantStorage(pageDir: string, realm: string): Promise<string | null> {
  if (typeof caches === 'undefined') return null
  const token = crypto.randomUUID()
  await (await caches.open(CACHE)).put(accessKey(pageDir), new Response(JSON.stringify({ realm, token })))
  return token
}

/** Close the storage of whichever realm was open; what is stored stays. */
export async function revokeStorage(pageDir: string): Promise<void> {
  if (typeof caches === 'undefined') return
  await (await caches.open(CACHE)).delete(accessKey(pageDir))
}

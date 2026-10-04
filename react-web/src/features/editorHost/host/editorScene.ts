// The editor's super-user scene, served by the page's service worker from bytes checked here.

import { CID_PATTERN, contentCid } from './cid'

// service_worker.js reads it; scenes cannot write it (sandbox_worker.js deletes `caches`)
const CACHE = 'dcl-editor-scene-v1'

async function verified(url: string, hash: string): Promise<Uint8Array<ArrayBuffer>> {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`the editor scene file ${hash} answered ${res.status}`)
  const bytes = new Uint8Array(await res.arrayBuffer())
  if ((await contentCid(bytes)) !== hash) throw new Error(`the editor scene file ${hash} does not match its hash`)
  return bytes
}

/** The id of the scene a static realm's `about` lists first. */
export function sceneIdFromAbout(about: unknown): string {
  const urn = (about as { configurations?: { scenesUrn?: unknown[] } } | null)?.configurations?.scenesUrn?.[0]
  const id = typeof urn === 'string' ? /^urn:decentraland:entity:([^?]+)/.exec(urn)?.[1] : undefined
  if (id == null) throw new Error('the editor scene realm lists no scene')
  return id
}

/** Fetch entity `entityId` and its files from `packageScene` (`<base>scene`), check each against
 *  its hash and store them for the service worker. Resolves to the realm url to spawn. */
export async function stageEditorScene(packageScene: string, entityId: string, pageDir: string): Promise<string> {
  if (!CID_PATTERN.test(entityId)) throw new Error(`the editor scene id ${entityId} is not a content hash`)
  if (navigator.serviceWorker?.controller == null) throw new Error('the page has no service worker to serve the editor scene')
  const realm = `${pageDir}editor-scene/${entityId}`
  const entity = await verified(`${packageScene}/${entityId}`, entityId)
  const { content } = JSON.parse(new TextDecoder().decode(entity)) as { content?: unknown }
  if (!Array.isArray(content)) throw new Error(`the editor scene ${entityId} lists no files`)
  const hashes = [...new Set(content.map((c: { hash?: unknown }) => String(c?.hash)))]
  const bad = hashes.find((hash) => !CID_PATTERN.test(hash))
  if (bad != null) throw new Error(`the editor scene names a file by ${bad}, not a content hash`)
  const files = await Promise.all(hashes.map(async (hash) => [hash, await verified(`${packageScene}/${hash}`, hash)] as const))

  const cache = await caches.open(CACHE)
  for (const key of await cache.keys()) if (!key.url.startsWith(`${realm}/`)) await cache.delete(key)
  await Promise.all([[entityId, entity] as const, ...files].map(([hash, bytes]) => cache.put(`${realm}/contents/${hash}`, new Response(bytes))))
  // last: the realm exists once every byte it names is there
  const about = {
    healthy: true,
    acceptingUsers: true,
    configurations: {
      networkId: 0,
      globalScenesUrn: [],
      scenesUrn: [`urn:decentraland:entity:${entityId}?=&baseUrl=${realm}/contents/`],
      realmName: 'editor-scene'
    },
    content: { healthy: true, publicUrl: `${realm}/contents` },
    comms: { healthy: true, protocol: 'v3', fixedAdapter: 'offline:offline' }
  }
  await cache.put(`${realm}/about`, new Response(JSON.stringify(about)))
  return realm
}

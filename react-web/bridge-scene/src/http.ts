// Shared data helpers: the catalyst base + two fetch primitives the domains build on.
//
// IMPORTANT: a system scene's `getRealm()` reports the scene's OWN realm (the local preview
// it was served from), NOT the world realm the engine is connected to — so it can't be used
// to locate the catalyst. We resolve the env from the ENGINE via `BevyApi.getRealmProvider()`
// and target the matching public catalyst (peer.decentraland.org / .zone).
import { BevyApi } from './bevy-api'

const PEER_ORG = 'https://peer.decentraland.org'
const PEER_ZONE = 'https://peer.decentraland.zone'

let cachedZone: boolean | null = null
async function resolveZone(): Promise<boolean> {
  if (cachedZone !== null) return cachedZone
  try {
    const provider = await BevyApi.getRealmProvider()
    cachedZone = typeof provider === 'string' && provider.includes('.zone')
  } catch {
    cachedZone = false
  }
  return cachedZone
}

/** Public catalyst base for the engine's realm (lambdas/content live under here). */
export async function catalystBase(): Promise<string> {
  return (await resolveZone()) ? PEER_ZONE : PEER_ORG
}

/** Whether the engine is on a .zone (test) realm — picks .org vs .zone service hosts. */
export async function isZone(): Promise<boolean> {
  return await resolveZone()
}

const META = JSON.stringify({})

// Catalyst/public GET. MUST go through the engine's kernelFetch, not the browser's fetch():
// the scene runs in the engine's COEP (cross-origin-isolated) context, where a raw fetch() to
// the catalyst is blocked (no CORP). kernelFetch performs the request natively in the engine.
export async function getJson<T>(url: string): Promise<T | undefined> {
  const r = await BevyApi.kernelFetch({
    url,
    init: { headers: { 'Content-Type': 'application/json' }, method: 'GET' },
    meta: META
  })
  if (!r.ok) throw new Error(`HTTP ${r.status}: ${r.statusText ?? r.body}`)
  if (!r.body) return undefined
  return JSON.parse(r.body) as T
}

export async function signed<T>(
  url: string,
  method: 'GET' | 'POST' | 'PUT' | 'DELETE' = 'GET',
  body?: object
): Promise<T | undefined> {
  const result = await BevyApi.kernelFetch({
    url,
    init: {
      headers: { 'Content-Type': 'application/json' },
      method,
      ...(body != null ? { body: JSON.stringify(body) } : {})
    },
    meta: META
  })
  if (!result.ok) throw new Error(`HTTP ${result.status}: ${result.statusText ?? result.body}`)
  if (!result.body) return undefined
  const parsed = JSON.parse(result.body) as { data?: T } & T
  return parsed.data ?? parsed
}

// Signed multipart/form-data POST/PUT with TEXT fields only (kernelFetch bodies are strings,
// so binary file parts like thumbnails can't go through here). The social-api community
// create/edit endpoints expect multipart, which is valid with text-only sections.
export async function signedForm<T>(
  url: string,
  method: 'POST' | 'PUT',
  fields: Record<string, string>
): Promise<T | undefined> {
  const boundary = '----DclBridgeFormBoundaryqZ1x9b7Kt3'
  let body = ''
  for (const [key, value] of Object.entries(fields)) {
    body += `--${boundary}\r\nContent-Disposition: form-data; name="${key}"\r\n\r\n${value}\r\n`
  }
  body += `--${boundary}--\r\n`
  const result = await BevyApi.kernelFetch({
    url,
    init: { headers: { 'Content-Type': `multipart/form-data; boundary=${boundary}` }, method, body },
    meta: META
  })
  if (!result.ok) throw new Error(`HTTP ${result.status}: ${result.statusText ?? result.body}`)
  if (!result.body) return undefined
  const parsed = JSON.parse(result.body) as { data?: T } & T
  return parsed.data ?? parsed
}

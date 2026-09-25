// Probe a realm's /about before booting the engine there (a ?realm= launch on web): launching
// against an unreachable realm strands the engine in a cryptic login failure. In-world travel needs
// no probe: the engine validates the destination before leaving the current realm.

import { serviceUrl } from './baseDomain'

export type RealmCheck = 'ok' | 'not-found' | 'unreachable'

const TIMEOUT_MS = 4000

export function realmAboutUrl(realm: string): string {
  const base = realm.endsWith('.dcl.eth') && !realm.startsWith('https://') ? `${serviceUrl('worldsServer')}/world/${realm}` : realm
  return `${base.replace(/\/+$/, '')}/about`
}

export async function checkRealm(realm: string): Promise<RealmCheck> {
  const timeout = new Promise<null>((resolve) => setTimeout(() => resolve(null), TIMEOUT_MS))
  try {
    const r = await Promise.race([fetch(realmAboutUrl(realm)), timeout])
    if (r?.ok) return 'ok'
    return r?.status === 404 ? 'not-found' : 'unreachable'
  } catch {
    return 'unreachable'
  }
}

export function realmCheckMessage(realm: string, result: Exclude<RealmCheck, 'ok'>): string {
  return result === 'not-found' ? `The world "${realm}" doesn't exist.` : `The world "${realm}" isn't reachable right now.`
}

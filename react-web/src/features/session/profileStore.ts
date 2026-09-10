// The HUD's one source for who a player is, keyed by address. Every surface that shows a name or a
// face for an address reads it from here — chat lines, the profile card, the passport — rather than
// each keeping its own copy with its own lifetime (the nearby roster is replaced wholesale every
// poll, so anything resolved from it alone degrades to a bare address the moment its owner walks
// off). What an entry holds grows as more is learned: the identity the nearby roster or friends
// list already carries, then the engine's own copy of the profile once the store has asked for it.
//
// Liveness is by subscription. `useProfile` subscribes its component to one address for as long as
// it is mounted, so a chat line on screen keeps its sender resolved however long ago they left; an
// entry nobody has shown for a while is swept. The engine announces profile changes (the
// `profileChanged` stream); an entry someone is still showing is re-read, the rest are dropped.
import { useCallback, useSyncExternalStore } from 'react'
import type { Profile } from '../../engine/protocol'

/** Addresses are matched lowercased: the same wallet reaches the page in either case. */
export const profileKey = (address: string): string => address.toLowerCase()

/** Identity a list already carries (the nearby roster, the friends list): enough to label a player
 *  before the engine's profile has been asked for. */
export interface ProfileSeed {
  address: string
  name: string
  picture?: string
}

interface Entry {
  /** The best known: the engine's profile once fetched, a seeded subset until then. */
  profile: Profile | undefined
  /** `profile` is the engine's copy (a `userProfile` reply): complete, and versioned. */
  full: boolean
  /** The passport's extras (badges, photos, equipped items) were fetched, so a re-read keeps them. */
  extras: boolean
  /** When the engine last answered that it holds nothing for this address. */
  missingAt: number | undefined
  pending: boolean
  subscribers: number
  /** When the last subscriber left, or a list last vouched for the address: the eviction clock. */
  idleSince: number
}

/** How long an entry nobody shows is kept. Generous on purpose: a re-read costs a bridge round trip
 *  against the engine's cache, and a player who steps out of range and back should not flicker. */
const IDLE_TTL = 120_000
/** How long a "no profile" answer stands before a subscriber asks again. */
const MISSING_TTL = 60_000
const SWEEP_EVERY = 30_000

const EXTRAS = ['badges', 'photos', 'equippedWearables', 'equippedEmotes'] as const

const entries = new Map<string, Entry>()
const listeners = new Map<string, Set<() => void>>()
let request: (address: string, extras: boolean) => void = () => {}
let lastSweep = 0

/** Only wallet addresses are worth asking the engine about — not the mock's 'You', nor a system line's ''. */
const isAddress = (key: string): boolean => key.startsWith('0x')
const notify = (key: string): void => listeners.get(key)?.forEach((l) => l())

function entry(key: string): Entry {
  let e = entries.get(key)
  if (e == null) {
    e = { profile: undefined, full: false, extras: false, missingAt: undefined, pending: false, subscribers: 0, idleSince: Date.now() }
    entries.set(key, e)
  }
  return e
}

function sweep(now: number): void {
  if (now - lastSweep < SWEEP_EVERY) return
  lastSweep = now
  for (const [key, e] of entries) {
    if (e.subscribers === 0 && !e.pending && now - e.idleSince > IDLE_TTL) {
      entries.delete(key)
      listeners.delete(key)
    }
  }
}

/** Ask the engine for an address nobody has asked about yet (or whose "nothing" answer has aged out).
 *  One request per address however many subscribers arrive while it's in flight. */
function ensureFetched(key: string, now: number): void {
  const e = entries.get(key)
  if (e == null || e.full || e.pending || !isAddress(key)) return
  if (e.missingAt != null && now - e.missingAt < MISSING_TTL) return
  e.pending = true
  request(key, false)
}

/** Where the store's requests go — the session installs the bridge once its driver is up. */
export function setProfileRequester(fn: (address: string, extras: boolean) => void): void {
  request = fn
}

export function subscribeProfile(key: string, cb: () => void): () => void {
  let set = listeners.get(key)
  if (set == null) {
    set = new Set()
    listeners.set(key, set)
  }
  set.add(cb)
  const e = entry(key)
  e.subscribers += 1
  const now = Date.now()
  ensureFetched(key, now)
  sweep(now)
  return () => {
    set.delete(cb)
    const cur = entries.get(key)
    if (cur == null) return
    cur.subscribers = Math.max(0, cur.subscribers - 1)
    if (cur.subscribers === 0) cur.idleSince = Date.now()
  }
}

/** What the store knows about an address, kept current for as long as the component is mounted.
 *  `undefined` until anything is known: callers fall back to the address itself. */
export function useProfile(address: string): Profile | undefined {
  const key = profileKey(address)
  const subscribe = useCallback((cb: () => void) => subscribeProfile(key, cb), [key])
  return useSyncExternalStore(subscribe, () => entries.get(key)?.profile)
}

/** A one-off read for code outside render (a click handler); no subscription, no fetch. */
export const peekProfile = (address: string): Profile | undefined => entries.get(profileKey(address))?.profile

const looksClaimed = (name: string): boolean => !name.includes('#') && !/^0x[0-9a-f]+$/i.test(name)

/** Identity a list already carries. Never overwrites the engine's copy; keeps an entry warm while
 *  the list keeps vouching for it (a nearby player stays resolvable until well after they leave). */
export function seedProfiles(seeds: readonly ProfileSeed[]): void {
  const now = Date.now()
  for (const seed of seeds) {
    if (seed.address === '' || seed.name.trim() === '') continue
    const key = profileKey(seed.address)
    const e = entry(key)
    if (e.subscribers === 0) e.idleSince = now
    if (e.full) continue
    const prev = e.profile
    const picture = seed.picture ?? prev?.picture
    if (prev != null && prev.name === seed.name && prev.picture === picture) continue
    e.profile = { address: seed.address, name: seed.name, picture, hasClaimedName: looksClaimed(seed.name), isGuest: prev?.isGuest ?? false }
    notify(key)
  }
  sweep(now)
}

/** A `userProfile` reply (or the local player's own `profile`). `null`: the engine holds nothing. */
export function receiveProfile(address: string, profile: Profile | null): void {
  const key = profileKey(address)
  const e = entry(key)
  e.pending = false
  if (profile == null) {
    e.missingAt = Date.now()
    return
  }
  e.missingAt = undefined
  const prev = e.profile
  const next: Profile = { ...profile }
  if (prev != null) {
    // The engine's copy replaces what a list seeded, but only where it says something: a profile
    // with no snapshot must not blank a face the friends service supplied, and vice versa when
    // that service stops sending faces — either way the row keeps the one it has.
    if (next.name === '' || next.name.toLowerCase() === key) next.name = prev.name
    if (next.picture == null) next.picture = prev.picture
    // A plain identity reply carries no badges/photos/equipped items; a passport's extras survive
    // the re-reads that follow. Everything else is the engine's word, including a cleared field.
    for (const k of EXTRAS) if (next[k] === undefined && prev[k] !== undefined) Object.assign(next, { [k]: prev[k] })
  }
  e.profile = next
  e.full = true
  notify(key)
  sweep(Date.now())
}

/** Change what's held for an address without a round trip — the optimistic own-profile edit. */
export function updateProfile(address: string, fn: (prev: Profile | undefined) => Profile | undefined): void {
  const key = profileKey(address)
  const e = entry(key)
  e.profile = fn(e.profile)
  notify(key)
}

/** The passport wants the whole thing: badges, photos, equipped items. Always re-read on open, as
 *  the plain identity fetch never brings those. */
export function requestPassport(address: string): void {
  const key = profileKey(address)
  if (!isAddress(key)) return
  const e = entry(key)
  e.extras = true
  e.pending = true
  e.missingAt = undefined
  request(key, true)
}

/** The engine holds a new version of a profile. Re-read it if someone is showing it and what's held
 *  is older; drop it if nobody is (the next subscriber fetches fresh); ignore an address never seen. */
export function profileChanged(address: string, version: number): void {
  const key = profileKey(address)
  const e = entries.get(key)
  if (e == null) return
  if (e.subscribers === 0) {
    entries.delete(key)
    listeners.delete(key)
    return
  }
  if (e.full && e.profile?.version != null && e.profile.version >= version) return
  e.pending = true
  e.missingAt = undefined
  request(key, e.extras)
}

/** Tests: back to empty, requests to nowhere. */
export function resetProfileStore(): void {
  entries.clear()
  listeners.clear()
  request = () => {}
  lastSweep = 0
}

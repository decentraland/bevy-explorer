// Outcome of each friend action as the bridge reports it, for surfaces that wait on one (the
// request popups show "Friend Request Sent" only once the service accepted it).
import type { FriendAction } from '../../engine/protocol'

export interface FriendEvent {
  op: FriendAction
  address: string
  ok: boolean
}

const subs = new Set<(e: FriendEvent) => void>()

export function onFriendEvent(fn: (e: FriendEvent) => void): () => void {
  subs.add(fn)
  return () => subs.delete(fn)
}

export function emitFriendEvent(e: FriendEvent): void {
  subs.forEach((fn) => fn(e))
}

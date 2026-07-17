import type { EngineSession } from './useEngineSession'

/** Resolve a user's display name + avatar by address from whatever the session already knows (the
 *  nearby roster, the friends/requests lists, then any fetched passport). Not found → the raw address,
 *  no avatar. Shared by the profile card and the passport, both of which open by address alone. */
export function resolveIdentity(session: EngineSession, userId: string): { name: string; picture?: string } {
  const a = userId.toLowerCase()
  const found =
    session.chat.members.find((m) => m.address.toLowerCase() === a) ??
    session.friends.list.find((f) => f.address.toLowerCase() === a) ??
    session.friends.received.find((r) => r.address.toLowerCase() === a) ??
    session.friends.sent.find((r) => r.address.toLowerCase() === a) ??
    session.userProfiles[a] ??
    undefined
  return { name: found?.name ?? userId, picture: found?.picture ?? undefined }
}

// Relationship of the local user to another user — drives the profile card's friend CTA + Block/Unblock.
export type Relationship = 'none' | 'requested' | 'incoming' | 'friend' | 'blocked'

/** The structural slice of `session.friends` needed to derive a relationship. */
export interface FriendsSlice {
  list: { address: string }[]
  received: { address: string }[]
  sent: { address: string }[]
  blocked: string[]
}

export function relationshipOf(friends: FriendsSlice, address: string): Relationship {
  const a = address.toLowerCase()
  if (friends.blocked.some((b) => b.toLowerCase() === a)) return 'blocked'
  if (friends.list.some((f) => f.address.toLowerCase() === a)) return 'friend'
  if (friends.received.some((r) => r.address.toLowerCase() === a)) return 'incoming'
  if (friends.sent.some((r) => r.address.toLowerCase() === a)) return 'requested'
  return 'none'
}

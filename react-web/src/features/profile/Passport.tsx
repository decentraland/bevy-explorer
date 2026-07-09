// Smart wrapper for the full-screen passport: resolves a user by address from the session, kicks off
// the rich-profile fetch, and renders the presentational ProfilePassport. Opened as a popup via
// openPassport() (from the profile card's "View Passport" and the sidebar's own profile), so it lives
// in the HUD-wide popup layer and reads the session via useSession() like the profile card.
import { useEffect } from 'react'
import { openPopup } from '../../design'
import { useSession } from '../session/SessionContext'
import { resolveIdentity } from '../session/resolveIdentity'
import { relationshipOf } from '../../lib/relationship'
import { ProfilePassport } from './ProfilePassport'
import type { Profile } from '../../engine/protocol'

export function Passport({ userId, onClose }: { userId: string; onClose: () => void }): React.JSX.Element {
  const session = useSession()
  const { requestUserProfile } = session
  // Fetch the rich profile (badges/photos/about) on open; render identity-only until it lands.
  useEffect(() => {
    requestUserProfile(userId)
  }, [requestUserProfile, userId])

  const a = userId.toLowerCase()
  const isSelf = !!session.profile.data && session.profile.data.address.toLowerCase() === a
  const { name, picture } = resolveIdentity(session, userId)
  const profile: Profile =
    session.userProfiles[a] ??
    (isSelf && session.profile.data
      ? session.profile.data
      : {
          address: userId,
          name,
          picture,
          hasClaimedName: !name.includes('#') && !/^0x[0-9a-f]+$/i.test(name),
          isGuest: false
        })

  return (
    <ProfilePassport
      profile={profile}
      isSelf={isSelf}
      relationship={relationshipOf(session.friends, userId)}
      onAddFriend={(address) => session.friends.act('request', address)}
      onClose={onClose}
    />
  )
}

// Single-instance (like openProfileCard): a new open closes the previous passport first.
let closeCurrent: (() => void) | null = null

/** Open a user's full-screen passport as a popup. */
export function openPassport(userId: string): () => void {
  closeCurrent?.()
  const handle = openPopup((close) => (
    <Passport
      userId={userId}
      onClose={() => {
        close()
        if (closeCurrent === handle) closeCurrent = null
      }}
    />
  ))
  closeCurrent = handle
  return handle
}

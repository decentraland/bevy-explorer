// Smart wrapper for the world profile card: resolves a user by address from the session and renders
// the presentational card. Opened as a popup via openProfileCard() (see the avatarClick handler in
// useEngineSession); because <PopupHost/> is mounted inside Hud's <SessionProvider>, the popup can
// read the session with useSession() even though it renders through a portal.
import { openPopup } from '../../design'
import { relationshipOf } from '../../lib/relationship'
import { useSession } from '../session/SessionContext'
import { ProfileCardPresentation, type ChatUser } from '../chat/ProfileCardPresentation'

export function ProfileCard({
  userId,
  x,
  y,
  onClose
}: {
  userId: string
  x: number
  y: number
  onClose: () => void
}): React.JSX.Element {
  const session = useSession()
  // Resolve the identity by address from whatever the session already knows: the nearby roster, the
  // friends/requests lists, then any fetched passport. Not found anywhere → the raw address, no avatar.
  const a = userId.toLowerCase()
  const found =
    session.chat.members.find((m) => m.address.toLowerCase() === a) ??
    session.friends.list.find((f) => f.address.toLowerCase() === a) ??
    session.friends.received.find((r) => r.address.toLowerCase() === a) ??
    session.friends.sent.find((r) => r.address.toLowerCase() === a) ??
    session.userProfiles[a] ??
    undefined
  const user: ChatUser = { address: userId, name: found?.name ?? userId, picture: found?.picture ?? undefined }
  return (
    <ProfileCardPresentation
      user={user}
      x={x}
      y={y}
      me={session.profile.data}
      relationship={relationshipOf(session.friends, userId)}
      onFriendAction={session.friends.act}
      onMention={session.chat.mention}
      onViewProfile={session.openPassport}
      onClose={onClose}
    />
  )
}

// Single-instance (matches the old worldCard "replace" semantics): a new click closes the previous
// card before opening, so two rapid avatarClicks don't stack two popups.
let closeCurrent: (() => void) | null = null

/** Open the world profile card as a popup, anchored at the given screen coords. */
export function openProfileCard(userId: string, x: number, y: number): () => void {
  closeCurrent?.()
  const handle = openPopup((close) => (
    <ProfileCard
      userId={userId}
      x={x}
      y={y}
      onClose={() => {
        close()
        if (closeCurrent === handle) closeCurrent = null
      }}
    />
  ))
  closeCurrent = handle
  return handle
}

// Smart wrapper for the world profile card: resolves a user by address from the session and renders
// the presentational card. Opened as a popup via openProfileCard() (see the avatarClick handler in
// useEngineSession); because <PopupHost/> is mounted inside Hud's <SessionProvider>, the popup can
// read the session with useSession() even though it renders through a portal.
import { openPopup } from '../../design'
import { relationshipOf } from '../../lib/relationship'
import { useSession } from '../session/SessionContext'
import { resolveIdentity } from '../session/resolveIdentity'
import { openPassport } from '../profile/Passport'
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
  const { name, picture } = resolveIdentity(session, userId)
  const user: ChatUser = { address: userId, name, picture }
  return (
    <ProfileCardPresentation
      user={user}
      x={x}
      y={y}
      me={session.profile.data}
      relationship={relationshipOf(session.friends, userId)}
      onFriendAction={session.friends.act}
      onMention={session.chat.mention}
      onViewProfile={() => openPassport(userId)}
      onClose={onClose}
    />
  )
}

/** Open the world profile card as a popup, anchored at the given screen coords. */
export function openProfileCard(userId: string, x: number, y: number): () => void {
  return openPopup((close) => <ProfileCard userId={userId} x={x} y={y} onClose={close} />, { dim: false }) // anchored popover, no scrim dim
}

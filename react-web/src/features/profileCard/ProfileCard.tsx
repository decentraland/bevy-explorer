// Smart wrapper for the world profile card: resolves a user by address from the profile store and renders
// the presentational card. Opened as a popup via openProfileCard() (see the avatarClick handler in
// useEngineSession); because <PopupHost/> is mounted inside Hud's <SessionProvider>, the popup can
// read the session with useSession() even though it renders through a portal.
import { openPopup } from '../../design'
import { relationshipOf } from '../../lib/relationship'
import { useSession } from '../session/SessionContext'
import { useProfile } from '../session/profileStore'
import { openPassport } from '../profile/Passport'
import { ProfileCardPresentation, type ChatUser, type MenuContext } from '../chat/ProfileCardPresentation'
import { confirmBlock, confirmUnfriend, reportUser } from '../friends/friendDialogs'
import { openFriendRequest } from '../friends/FriendRequestPopup'
import { splitName } from '../../lib/identity'

export function ProfileCard({
  userId,
  x,
  y,
  above,
  context,
  onClose
}: {
  userId: string
  x: number
  y: number
  above?: boolean
  context?: MenuContext
  onClose: () => void
}): React.JSX.Element {
  const session = useSession()
  const known = useProfile(userId)
  const user: ChatUser = { address: userId, name: known?.name ?? userId, picture: known?.picture, claimed: known?.hasClaimedName, nameColor: known?.nameColor }
  const act = session.friends.act
  return (
    <ProfileCardPresentation
      user={user}
      x={x}
      y={y}
      above={above}
      context={context}
      me={session.profile.data}
      relationship={relationshipOf(session.friends, userId)}
      onAddFriend={(u) => openFriendRequest('send', u)}
      onUnfriend={(u) => void confirmUnfriend(u).then((ok) => ok && act('delete', u.address))}
      onCancelRequest={(u) => act('cancel', u.address)}
      onAcceptRequest={(u) => openFriendRequest('accept', u)}
      onBlock={(u) => void confirmBlock(splitName(u.name).base).then((ok) => ok && act('block', u.address))}
      onReport={(u) => void reportUser(session.profile.data?.address, u.address)}
      onMention={session.chat.mention}
      onViewProfile={() => openPassport(userId)}
      onClose={onClose}
    />
  )
}

export interface ProfileCardOptions {
  /** (x, y) is the card's bottom-left; it grows upward (a row's menu button). */
  above?: boolean
  context?: MenuContext
  /** Runs when the card closes by any path. */
  onClose?: () => void
}

/** Open the world profile card as a popup, anchored at the given screen coords. */
export function openProfileCard(userId: string, x: number, y: number, opts: ProfileCardOptions = {}): () => void {
  return openPopup(
    (close) => <ProfileCard userId={userId} x={x} y={y} above={opts.above} context={opts.context} onClose={close} />,
    { dim: false, onClose: opts.onClose }
  ) // anchored popover, no scrim dim
}

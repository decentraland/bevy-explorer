// Profile viewer — the popover the SDK7 chat opened (PROFILE_MENU) when you clicked
// a sender's name/avatar or an @mention. A mini passport (avatar, name, address) plus
// quick actions (Add Friend, Mention). Anchored at the click point.

import { Avatar, Button } from '../../design'
import { nameColor, shortAddr, splitName } from '../../lib/identity'
import styles from './ProfileCard.module.css'

export interface ChatUser {
  address: string
  name: string
  picture?: string
}

export function ProfileCard({
  user,
  x,
  y,
  me,
  onAddFriend,
  onMention,
  onClose
}: {
  user: ChatUser
  x: number
  y: number
  me?: { address?: string } | null
  onAddFriend?: (address: string) => void
  onMention?: (name: string) => void
  onClose: () => void
}): React.JSX.Element {
  const isMe = !!me?.address && !!user.address && me.address.toLowerCase() === user.address.toLowerCase()
  const { base, tag } = splitName(user.name)
  const color = nameColor(user.address || user.name)
  // Keep the card on-screen (it's ~240px wide / ~200px tall).
  const left = Math.min(x, window.innerWidth - 260)
  const top = Math.min(y, window.innerHeight - 220)

  return (
    <>
      <div className={styles.scrim} onClick={onClose} />
      <div className={styles.card} style={{ left, top }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Profile">
        <Avatar src={user.picture} name={base} color={color} size={56} status="online" />
        <div className={styles.name} style={{ color }}>
          {base}
          {tag && <span className={styles.tag}>{tag}</span>}
        </div>
        {user.address && <div className={styles.addr}>{shortAddr(user.address)}</div>}
        {!isMe && (
          <div className={styles.actions}>
            {user.address && onAddFriend && (
              <Button size="sm" onClick={() => { onAddFriend(user.address); onClose() }}>
                Add Friend
              </Button>
            )}
            {onMention && (
              <Button size="sm" variant="secondary" onClick={() => { onMention(base); onClose() }}>
                Mention
              </Button>
            )}
          </div>
        )}
      </div>
    </>
  )
}

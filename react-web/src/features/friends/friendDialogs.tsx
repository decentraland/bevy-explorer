// Confirmations shared by the friends panel, the profile card and the passport, worded like the
// reference client.
import { Avatar, showConfirm } from '../../design'
import { BASE_DOMAIN } from '../../lib/baseDomain'
import { userColor } from '../../lib/identity'
import styles from './FriendRequestPopup.module.css'

export interface DialogUser {
  address: string
  name: string
  picture?: string
}

export function confirmUnfriend(user: DialogUser): Promise<boolean> {
  return showConfirm({
    title: `Are you sure you want to unfriend ${user.name}?`,
    body: (
      <div className={styles.centerAvatar}>
        <Avatar src={user.picture} name={user.name} color={userColor(user.address, user.name)} size={72} />
      </div>
    ),
    confirmLabel: 'Unfriend'
  })
}

export function confirmBlock(name: string): Promise<boolean> {
  return showConfirm({
    title: `Are you sure you want to block ${name}?`,
    body: 'If you block someone in Decentraland, you will no longer see their avatar in-world, and you will not be able to send friend requests or messages to each other. You will also not see each other’s names or messages in public chats.',
    confirmLabel: 'BLOCK'
  })
}

export function confirmUnblock(name: string): Promise<boolean> {
  return showConfirm({
    title: `Are you sure you want to unblock ${name}?`,
    body: 'If you unblock someone, you will see their avatar in-world, and you will be able to send friend requests and messages to each other in public or private chats.',
    confirmLabel: 'UNBLOCK'
  })
}

/** Confirm, then open the report form for `reported` in a new tab. */
export async function reportUser(me: string | undefined, reported: string): Promise<void> {
  const ok = await showConfirm({
    title: 'You will be redirected to a web form',
    body: 'Fill out the form with as much detail as you can, including evidence of what happened.',
    confirmLabel: 'Report'
  })
  if (!ok) return
  const url = `https://${BASE_DOMAIN}/report/players?player_address=${encodeURIComponent(me ?? '')}&reported_address=${encodeURIComponent(reported)}`
  window.open(url, '_blank', 'noopener')
}

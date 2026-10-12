import { Avatar, showToast } from '../../design'
import type { FriendAction } from '../../engine/protocol'
import { shortAddr, splitName } from '../../lib/identity'
import { knownUserColor, peekProfile } from '../session/profileStore'

const VERB: Record<FriendAction, string> = {
  request: 'sending a friend request to',
  accept: 'accepting the friend request from',
  reject: 'declining the friend request from',
  cancel: 'cancelling the friend request to',
  delete: 'removing',
  block: 'blocking',
  unblock: 'unblocking'
}

function nameOf(address: string): string {
  return splitName(peekProfile(address)?.name ?? '').base || shortAddr(address)
}

/** Block/unblock report success; any failed action reports the failure. */
export function toastFriendResult(op: FriendAction, address: string, ok: boolean): void {
  const name = nameOf(address)
  if (!ok) {
    showToast(
      <>
        Something went wrong while {VERB[op]} <b>{name}</b>
      </>,
      { tone: 'error' }
    )
    return
  }
  if (op === 'block' || op === 'unblock') {
    showToast(
      <>
        User <b>{name}</b> has been successfully {op === 'block' ? 'blocked' : 'unblocked'}.
      </>
    )
  }
}

export function toastFriendOnline(address: string): void {
  const p = peekProfile(address)
  const name = nameOf(address)
  showToast(
    <>
      <b style={{ color: knownUserColor(address, name) }}>{name}</b> Is Online
    </>,
    { icon: <Avatar src={p?.picture} name={name} color={knownUserColor(address, name)} size={28} /> }
  )
}

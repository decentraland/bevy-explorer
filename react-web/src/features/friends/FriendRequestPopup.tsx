// Friend request popup: send one (with an intro message), or look at one received / sent. The
// result screens ("Friend Request Sent To", "You And X Are Now Friends!") show only once the
// friends service accepted the action, then close by themselves.
import { useEffect, useRef, useState } from 'react'
import { Avatar, Button, ModalShell, Spinner, TextArea, openPopup } from '../../design'
import { nameColor, splitName } from '../../lib/identity'
import { useSession } from '../session/SessionContext'
import { onFriendEvent } from './friendEvents'
import styles from './FriendRequestPopup.module.css'

export type RequestMode = 'send' | 'received' | 'sent' | 'accept'

export interface RequestUser {
  address: string
  name: string
  picture?: string
  message?: string
  createdAt?: number
}

const MONTHS = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC']
const shortDate = (ts?: number): string => {
  if (!ts) return ''
  const d = new Date(ts)
  return `${MONTHS[d.getMonth()]} ${String(d.getDate()).padStart(2, '0')}`
}

function Mutuals({ address }: { address: string }): React.JSX.Element | null {
  const { friends } = useSession()
  const list = friends.mutuals[address.toLowerCase()]
  const load = friends.loadMutuals
  useEffect(() => load(address), [address, load])
  if (!list || list.length === 0) return null
  return (
    <div className={styles.mutuals}>
      {list.slice(0, 3).map((m) => (
        <Avatar key={m.address} src={m.picture} name={m.name} color={nameColor(m.address)} size={24} className={styles.mutualFace} />
      ))}
      <span>{list.length} Mutual</span>
    </div>
  )
}

function Who({ user }: { user: RequestUser }): React.JSX.Element {
  const { base, tag } = splitName(user.name)
  return (
    <div className={styles.who}>
      <Avatar src={user.picture} name={base} color={nameColor(user.address)} size={72} />
      <span className={styles.name} style={{ color: nameColor(user.address) }}>
        {base}
        {tag && <span className={styles.tag}>{tag}</span>}
      </span>
    </div>
  )
}

function FriendRequestPopup({ mode, user, onClose }: { mode: RequestMode; user: RequestUser; onClose: () => void }): React.JSX.Element {
  const session = useSession()
  const { friends } = session
  const me = session.profile.data
  const [message, setMessage] = useState('')
  const [done, setDone] = useState<'sent' | 'friends' | null>(null)
  const [cancelArmed, setCancelArmed] = useState(false)
  const key = (op: string): string => `${op}:${user.address.toLowerCase()}`
  const busy = friends.pending.has(key('request')) || friends.pending.has(key('accept')) || friends.pending.has(key('cancel'))

  useEffect(
    () =>
      onFriendEvent((e) => {
        if (e.address.toLowerCase() !== user.address.toLowerCase()) return
        if (!e.ok) {
          if (mode === 'accept') onClose() // the failure toast says why
          return
        }
        if (e.op === 'request') setDone('sent')
        else if (e.op === 'accept') setDone('friends')
        else if (e.op === 'reject' || e.op === 'cancel') onClose()
      }),
    [user.address, onClose, mode]
  )
  // 'accept' is a one-shot: accept on open, then show the result.
  const accepted = useRef(false)
  const act = friends.act
  useEffect(() => {
    if (mode !== 'accept' || accepted.current) return
    accepted.current = true
    act('accept', user.address)
  }, [mode, act, user.address])
  useEffect(() => {
    if (done == null) return
    const t = setTimeout(onClose, 5000)
    return () => clearTimeout(t)
  }, [done, onClose])
  // The sent-request cancel is two-step: a warning first, the real cancel after 3s.
  useEffect(() => {
    if (!cancelArmed) return
    const t = setTimeout(() => setCancelArmed(false), 3000)
    return () => clearTimeout(t)
  }, [cancelArmed])

  const { base } = splitName(user.name)
  if (done === 'sent') {
    return (
      <ModalShell onClose={onClose} title={`Friend Request Sent To ${base}`} centeredTitle>
        <Who user={user} />
      </ModalShell>
    )
  }
  if (done === 'friends') {
    return (
      <ModalShell onClose={onClose} title={`You And ${base} Are Now Friends!`} centeredTitle>
        <div className={styles.pair}>
          <Avatar src={me?.picture} name={me?.name ?? ''} color={nameColor(me?.address ?? '')} size={72} />
          <Avatar src={user.picture} name={base} color={nameColor(user.address)} size={72} />
        </div>
      </ModalShell>
    )
  }

  if (mode === 'accept') {
    return (
      <ModalShell onClose={onClose} title="Accepting Friend Request" centeredTitle>
        <div className={styles.pair}>
          <Spinner />
        </div>
      </ModalShell>
    )
  }

  if (mode === 'send') {
    return (
      <ModalShell
        onClose={onClose}
        title="Send Friend Request To"
        centeredTitle
        actionsEqual
        actions={
          <>
            <Button variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button disabled={busy} onClick={() => friends.act('request', user.address, message.trim() || undefined)}>
              Send
            </Button>
          </>
        }
      >
        <Who user={user} />
        <Mutuals address={user.address} />
        <TextArea value={message} onChange={setMessage} maxLength={140} counter rows={3} placeholder="Write an introduction message" className={styles.message} />
      </ModalShell>
    )
  }

  const received = mode === 'received'
  return (
    <ModalShell
      onClose={onClose}
      title={received ? 'Friend Request Received' : 'Friend Request Sent'}
      centeredTitle
      actionsEqual={received}
      actions={
        received ? (
          <>
            <Button variant="secondary" disabled={busy} onClick={() => friends.act('reject', user.address)}>
              Reject
            </Button>
            <Button disabled={busy} onClick={() => friends.act('accept', user.address)}>
              Accept
            </Button>
          </>
        ) : cancelArmed ? (
          <Button disabled={busy} onClick={() => friends.act('cancel', user.address)}>
            Cancel Request
          </Button>
        ) : (
          <Button variant="secondary" onClick={() => setCancelArmed(true)}>
            Cancel Request
          </Button>
        )
      }
    >
      <div className={styles.date}>{shortDate(user.createdAt)}</div>
      <Who user={user} />
      <Mutuals address={user.address} />
      {user.message && (
        <p className={styles.quote}>
          <b>{received ? `${base}:` : 'You:'}</b> {user.message}
        </p>
      )}
      {!received && cancelArmed && <p className={styles.warn}>This will cancel your friend request. Press Cancel Request again to confirm.</p>}
    </ModalShell>
  )
}

export function openFriendRequest(mode: RequestMode, user: RequestUser): () => void {
  return openPopup((close) => <FriendRequestPopup mode={mode} user={user} onClose={close} />)
}

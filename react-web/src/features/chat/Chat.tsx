// React DOM chat — Explorer 2.0 design, three states:
//   • collapsed (hidden): a borderless translucent input bar; focusing it opens chat
//   • open + idle (not hovered/focused): translucent — bubbles float over the world
//   • open + active (hover/focus): full solid panel — navbar, emoji, members, borders
// Incoming messages come from the bridge getChatStream relay; sends go via BevyApi.sendChat.

import { Fragment, memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ChatLine, ChatState } from '../session/useEngineSession'
import type { NearbyMember } from '../../engine/protocol'
import { Avatar, ControlButton, DclLogo, MaskIcon, VerifiedBadge, VoiceBars } from '../../design'
import { EmojiPicker } from './EmojiPicker'
import { searchByShortcode, type Emoji } from './emojiData'
import { MessageText, mentionsMe, buildNameIndex } from './chatText'
import { type ChatUser } from './ProfileCardPresentation'
import { openProfileCard } from '../profileCard/ProfileCard'
import { knownUserColor, peekProfile, useProfile } from '../session/profileStore'
import { isCancelKey } from '../../lib/bindingLabels'
import { hudInsetRef } from '../../lib/hudInset'
import playersIcon from '../../assets/chat/players.png'
import closeIcon from '../../assets/chat/close-thin.png'
import newTag from '../../assets/chat/new-tag.png'
import arrowLeftIcon from '../../assets/chat/arrow-left.png'
import styles from './Chat.module.css'

const MAX_LEN = 500
const ADDRESS_RE = /^0x[0-9a-fA-F]{6,}$/

const SYSTEM_COLOR = 'var(--chat-system)'

function isSystem(sender: string): boolean {
  return !sender || sender.toLowerCase() === 'system'
}

function shortAddr(s: string): string {
  return ADDRESS_RE.test(s) ? `${s.slice(0, 6)}…${s.slice(-4)}` : s
}

function displaySender(sender: string): string {
  if (isSystem(sender)) return 'DCL System'
  return shortAddr(sender)
}

function memberLabel(m: NearbyMember): string {
  return m.name.trim() ? m.name : shortAddr(m.address)
}

/** Split "Name#a1b2" into the colored base and a dimmer #tag. */
function splitName(label: string): { base: string; tag: string } {
  const i = label.indexOf('#')
  return i >= 0 ? { base: label.slice(0, i), tag: label.slice(i) } : { base: label, tag: '' }
}

function senderColor(sender: string, name: string): string {
  if (isSystem(sender)) return SYSTEM_COLOR
  return knownUserColor(sender, name)
}

type Suggestions =
  | { kind: 'emoji'; items: Emoji[]; start: number; end: number; sel: number }
  | { kind: 'mention'; items: NearbyMember[]; start: number; end: number; sel: number }

/** The ":emoji" or "@name" word ending at the caret, if any. */
export function tokenAtCaret(value: string, caret: number): { kind: 'emoji' | 'mention'; query: string; start: number } | null {
  const before = value.slice(0, caret)
  const m = before.match(/(?:^|\s)([@:])(\S*)$/)
  if (!m) return null
  const query = m[2]
  const start = caret - query.length - 1
  if (m[1] === ':') return /^[\w+-]{2,}$/.test(query) && !/https?$/i.test(before.slice(0, start)) ? { kind: 'emoji', query, start } : null
  return /^[A-Za-z0-9]{1,15}$/.test(query) ? { kind: 'mention', query, start } : null
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })
}

function dayKey(ts: number): string {
  const d = new Date(ts)
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`
}

function formatDay(ts: number): string {
  const key = dayKey(ts)
  const now = Date.now()
  if (key === dayKey(now)) return 'Today'
  if (key === dayKey(now - 86_400_000)) return 'Yesterday'
  const d = new Date(ts)
  const weekday = d.toLocaleDateString('en-US', { weekday: 'short' })
  const month = d.toLocaleDateString('en-US', { month: 'short' })
  const year = d.getFullYear() === new Date(now).getFullYear() ? '' : `, ${d.getFullYear()}`
  return `${weekday}, ${d.getDate()} ${month}${year}`
}

function CharRing({ len }: { len: number }): React.JSX.Element {
  const pct = Math.min(1, len / MAX_LEN)
  const r = 9
  const circ = 2 * Math.PI * r
  const color = pct >= 0.8 ? 'var(--brand)' : pct >= 0.5 ? 'var(--counter-half)' : 'var(--green)'
  return (
    <svg className={styles.ring} width="22" height="22" viewBox="0 0 22 22" aria-hidden="true">
      <circle cx="11" cy="11" r={r} fill="none" stroke="var(--ink-66)" strokeWidth="2" />
      <circle
        cx="11"
        cy="11"
        r={r}
        fill="none"
        stroke={color}
        strokeWidth="2"
        strokeDasharray={circ}
        strokeDashoffset={circ * (1 - pct)}
        transform="rotate(-90 11 11)"
      />
    </svg>
  )
}

export function DaySeparator({ ts }: { ts: number }): React.JSX.Element {
  return (
    <div className={styles.dayRow}>
      <span className={styles.dayPill}>{formatDay(ts)}</span>
    </div>
  )
}

function NewSeparator(): React.JSX.Element {
  return (
    <div className={styles.newRow} role="separator" aria-label="New messages">
      <img className={styles.newTag} src={newTag} alt="" />
    </div>
  )
}

const MSG_STYLES = { url: styles.url, mention: styles.mention, location: styles.location, world: styles.world }

export const ChatBubble = memo(function ChatBubble({
  line,
  members = [],
  me,
  onOpenProfile,
  onLocation,
  onVisitWorld,
  arrive = false
}: {
  line: ChatLine
  /** A live message (not history): fades in on mount. */
  arrive?: boolean
  members?: NearbyMember[]
  me?: { address?: string; name?: string } | null
  /** Open the profile viewer for a user, anchored at the click. */
  onOpenProfile?: (user: ChatUser, e: React.MouseEvent) => void
  /** A location link (x,y) in the message was clicked → teleport. */
  onLocation?: (x: number, y: number) => void
  /** A world name (e.g. boedo.dcl.eth) in the message was clicked → prompt to jump there. */
  onVisitWorld?: (name: string) => void
}): React.JSX.Element {
  // Resolved from the profile store for as long as the line is on screen: a sender who has since
  // left keeps their name and face, and a name change reaches every line they sent.
  const known = useProfile(line.sender)
  const system = isSystem(line.sender)
  const own = !system && me?.address != null && me.address.toLowerCase() === line.sender.toLowerCase()
  const name = known?.name != null && known.name !== '' ? known.name : displaySender(line.sender)
  const picture = known?.picture
  const color = senderColor(line.sender, name)
  const { base, tag } = splitName(name)
  const claimed = system || known?.hasClaimedName === true
  const sender: ChatUser = { address: line.sender, name, picture }
  const highlight = !own && mentionsMe(line.message, me ?? null, buildNameIndex(members))
  const clickable = !own && !system && onOpenProfile != null

  const openSender = (e: React.MouseEvent): void => {
    if (e.type === 'contextmenu') e.preventDefault()
    onOpenProfile?.(sender, e)
  }
  const onMention = (address: string, mname: string, e: React.MouseEvent): void => {
    if (e.type === 'contextmenu') e.preventDefault()
    const m = peekProfile(address)
    onOpenProfile?.({ address, name: m?.name ?? `@${mname}`, picture: m?.picture }, e)
  }

  const avatar = system ? (
    <DclLogo size={28} className={styles.systemAvatar} />
  ) : (
    <Avatar src={picture} name={name} color={color} size={28} framed className={styles.avatar} />
  )
  const nameRow = (
    <>
      {base}
      {!claimed && tag && <span className={styles.tag}>{tag}</span>}
      {claimed && <VerifiedBadge size={14} className={styles.badge} />}
    </>
  )

  return (
    <div className={`${styles.entry} ${own ? styles.own : ''} ${arrive ? styles.arrive : ''}`.trim()}>
      {clickable ? (
        <button type="button" className={styles.avatarBtn} aria-label={`View ${base}`} onClick={openSender} onContextMenu={openSender}>
          {avatar}
        </button>
      ) : (
        <span className={styles.avatarBtn}>{avatar}</span>
      )}
      <div className={`${styles.bubble} ${highlight ? styles.mentionMe : ''}`.trim()}>
        <div className={styles.content}>
          {clickable ? (
            <button type="button" className={styles.name} style={{ color }} onClick={openSender} onContextMenu={openSender}>
              {nameRow}
            </button>
          ) : (
            <span className={styles.name} style={{ color }}>
              {nameRow}
            </span>
          )}
          <span className={styles.text}>
            <MessageText text={line.message} members={members} styles={MSG_STYLES} onMention={onMention} onLocation={(x, y) => onLocation?.(x, y)} onWorld={onVisitWorld} />
          </span>
          <span className={styles.time}>{formatTime(line.ts)}</span>
        </div>
        <span className={styles.spacer} aria-hidden="true" />
      </div>
    </div>
  )
})

function MemberName({ member }: { member: NearbyMember }): React.JSX.Element {
  const known = useProfile(member.address)
  const { base, tag } = splitName(memberLabel(member))
  const claimed = known?.hasClaimedName ?? (tag === '' && member.name.trim() !== '')
  return (
    <span className={styles.memberName} style={{ color: senderColor(member.address, memberLabel(member)) }}>
      {base}
      {!claimed && tag && <span className={styles.memberTag}>{tag}</span>}
      {claimed && <VerifiedBadge size={14} className={styles.badge} />}
    </span>
  )
}

export function MemberRow({ member, speaking = false }: { member: NearbyMember; speaking?: boolean }): React.JSX.Element {
  const { base } = splitName(memberLabel(member))
  const color = senderColor(member.address, memberLabel(member))
  return (
    <div className={styles.memberRow}>
      <Avatar src={member.picture} name={base} color={color} size={40} framed status="online" dotPosition="top" />
      <div className={styles.memberInfo}>
        <MemberName member={member} />
        <span className={styles.memberStatus}>
          {speaking ? (
            <>
              <VoiceBars /> Speaking
            </>
          ) : (
            'Online'
          )}
        </span>
      </div>
    </div>
  )
}

function MembersOverlay({
  members,
  speaking,
  onBack,
  onClose
}: {
  members: NearbyMember[]
  speaking: ReadonlySet<string>
  onBack: () => void
  onClose: () => void
}): React.JSX.Element {
  return (
    <div className={styles.membersPanel}>
      <header className={`${styles.nav} ${styles.membersHeader}`}>
        <ControlButton variant="dark" aria-label="Back" onClick={onBack}>
          <MaskIcon src={arrowLeftIcon} size={12} className={styles.backIcon} />
        </ControlButton>
        <span className={styles.navTitle}>Nearby&nbsp; -</span>
        <span className={styles.membersCount}>
          <MaskIcon src={playersIcon} size={16} />
          <span className={styles.countNum}>{members.length}</span> Online
        </span>
        <ControlButton variant="dark" className={styles.close} aria-label="Close chat" onClick={onClose}>
          <MaskIcon src={closeIcon} size={10} />
        </ControlButton>
      </header>
      <div className={styles.membersList}>
        {members.length === 0 ? (
          <div className={styles.empty}>No one nearby</div>
        ) : (
          members.map((m) => <MemberRow key={m.address} member={m} speaking={speaking.has(m.address.toLowerCase())} />)
        )}
      </div>
    </div>
  )
}

export function Chat({
  chat,
  hidden = false,
  me,
  onTeleport,
  onVisitWorld
}: {
  chat: ChatState
  hidden?: boolean
  /** The local player (for @-me highlight + hiding self-actions in the viewer). */
  me?: { address?: string; name?: string } | null
  /** A location link (x,y) in a message was clicked. */
  onTeleport?: (x: number, y: number) => void
  /** A world name (e.g. boedo.dcl.eth) in a message was clicked → prompt to jump there. */
  onVisitWorld?: (name: string) => void
}): React.JSX.Element | null {
  const [draft, setDraft] = useState('')
  const [picker, setPicker] = useState(false)
  const [showMembers, setShowMembers] = useState(false)
  const [sug, setSug] = useState<Suggestions | null>(null)
  const [hovered, setHovered] = useState(false)
  const [focused, setFocused] = useState(false)
  const [dim, setDim] = useState(false)
  const listRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const pickerRef = useRef<HTMLDivElement>(null)

  const open = chat.open
  // "active" = the user is interacting → show the full solid panel + chrome.
  const active = open && (hovered || focused || picker)
  const bare = !active // collapsed or idle-open → borderless translucent input only

  // Lines already in the log when the list mounts are history; only later ones fade in.
  const lastId = chat.messages.length > 0 ? chat.messages[chat.messages.length - 1].id : -Infinity
  const liveFrom = useRef(lastId + 1)
  useEffect(() => {
    if (open) liveFrom.current = lastId + 1
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // Idle and unhovered, the messages dim after 10s; a new message or any interaction restores them.
  const IDLE_DIM_MS = 10_000
  useEffect(() => {
    setDim(false)
    if (!open || active) return
    const t = setTimeout(() => setDim(true), IDLE_DIM_MS)
    return () => clearTimeout(t)
  }, [open, active, lastId])

  const rows = useMemo(() => {
    const out: ({ kind: 'day'; ts: number; id: string } | { kind: 'msg'; line: ChatLine })[] = []
    let prev = ''
    for (const line of chat.messages) {
      const key = dayKey(line.ts)
      if (key !== prev) {
        out.push({ kind: 'day', ts: line.ts, id: `day-${key}` })
        prev = key
      }
      out.push({ kind: 'msg', line })
    }
    return out
  }, [chat.messages])

  // Opening/activating the chat always jumps to the latest message.
  useEffect(() => {
    if (!open) return
    const el = listRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [open, active])

  // A new message only auto-scrolls if the user was already near the bottom — otherwise it
  // would yank them away from history they scrolled up to read. Tracked via a live `scroll`
  // listener (not recomputed at message time) so a burst of several messages landing in one
  // React batch — which only re-renders once — still checks the position from just before the
  // burst, not the cumulative jump the batch itself causes.
  const NEAR_BOTTOM_PX = 80
  const nearBottomRef = useRef(true)
  // Messages from others that landed while scrolled up: counted on the scroll-to-bottom button,
  // and the first of them gets the NEW separator until the chat closes.
  const [unread, setUnread] = useState(0)
  const [newFrom, setNewFrom] = useState<number | null>(null)
  const shownUnread = useRef(0)
  if (unread > 0) shownUnread.current = unread
  const seenId = useRef(lastId)
  useEffect(() => {
    const el = listRef.current
    if (!el) return
    const onScroll = (): void => {
      nearBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX
      if (nearBottomRef.current) setUnread(0)
    }
    el.addEventListener('scroll', onScroll)
    return () => el.removeEventListener('scroll', onScroll)
  }, [open])
  useEffect(() => {
    if (open) return
    setUnread(0)
    setNewFrom(null)
    nearBottomRef.current = true
  }, [open])
  const lastSender = chat.messages.length > 0 ? chat.messages[chat.messages.length - 1].sender : ''
  const lastIsOwn = me?.address != null && lastSender.toLowerCase() === me.address.toLowerCase()
  useEffect(() => {
    const fresh = chat.messages.filter((l) => l.id > seenId.current)
    seenId.current = lastId
    if (!open) return
    const el = listRef.current
    if (el && (nearBottomRef.current || lastIsOwn)) {
      el.scrollTop = el.scrollHeight
      return
    }
    const mine = me?.address?.toLowerCase()
    const others = fresh.filter((l) => l.sender.toLowerCase() !== mine)
    if (others.length === 0) return
    setUnread((n) => n + others.length)
    setNewFrom((id) => id ?? others[0].id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat.messages, open])
  const scrollToBottom = (): void => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight, behavior: 'smooth' })
  }

  // Opening chat (sidebar icon, a queued mention, or Enter) focuses the input so it comes up in
  // the active/focused state, ready to type — matches Unity's "click chat → start typing". Also
  // reacts to focusTick alone: Enter while chat is already idle-open doesn't change `open`, so
  // the engine's "Chat" system action (see bridge-scene chat.ts) bumps this tick to force focus.
  useEffect(() => {
    if (open) inputRef.current?.focus()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, chat.focusTick])

  // Leaving the chat (click outside → not hovered/focused) resets the nearby-members
  // overlay, so re-entering shows messages — not the members list left open from before.
  useEffect(() => {
    if (!active) setShowMembers(false)
  }, [active])

  const openIfClosed = (): void => {
    if (!chat.open) chat.toggle()
  }

  // Profile viewer: clicking a name/avatar/@mention opens the shared profile card at the click.
  const openProfile = useCallback((user: ChatUser, e: React.MouseEvent): void => {
    openProfileCard(user.address, e.clientX, e.clientY)
  }, [])
  // The HUD passes fresh arrows each render; read them through refs so bubbles stay memoized.
  const handlers = useRef({ onTeleport, onVisitWorld })
  useEffect(() => {
    handlers.current = { onTeleport, onVisitWorld }
  })
  const teleport = useCallback((x: number, y: number) => handlers.current.onTeleport?.(x, y), [])
  const visitWorld = useCallback((name: string) => handlers.current.onVisitWorld?.(name), [])
  const hasVisitWorld = onVisitWorld != null
  // "Mention" from the viewer drops @name into the draft, ready to send.
  const insertMention = (name: string): void => {
    setDraft((d) => `${d.replace(/\s*$/, '')} @${name} `.trimStart())
    openIfClosed()
    inputRef.current?.focus()
  }
  // A mention queued from another surface (world/friends profile card) — drop it in and clear.
  useEffect(() => {
    if (!chat.pendingMention) return
    insertMention(chat.pendingMention)
    chat.consumeMention()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat.pendingMention])

  // Suggestions follow the word at the caret: ":sm" (2+ chars) lists emoji, "@a" (1+) nearby people.
  const suggestAt = (value: string, caret: number): void => {
    const word = tokenAtCaret(value, caret)
    if (word == null) return setSug(null)
    if (word.kind === 'emoji') {
      const items = searchByShortcode(word.query, 50)
      if (items.length > 0) return setSug({ kind: 'emoji', items, start: word.start, end: caret, sel: 0 })
      return setSug(null)
    }
    const q = word.query.toLowerCase()
    const items = chat.members.filter((m) => memberLabel(m).toLowerCase().includes(q))
    setSug(items.length > 0 ? { kind: 'mention', items, start: word.start, end: caret, sel: 0 } : null)
  }
  const updateDraft = (value: string, caret: number): void => {
    setDraft(value)
    suggestAt(value, caret)
  }

  const replaceRange = (start: number, end: number, text: string): void => {
    const next = (draft.slice(0, start) + text + draft.slice(end)).slice(0, MAX_LEN)
    const caret = Math.min(start + text.length, next.length)
    setDraft(next)
    setSug(null)
    requestAnimationFrame(() => {
      const el = inputRef.current
      if (!el) return
      el.focus()
      el.setSelectionRange(caret, caret)
    })
  }
  const accept = (s: Suggestions, i: number): void => {
    if (s.kind === 'emoji') replaceRange(s.start, s.end, s.items[i].emoji)
    else {
      const m = s.items[i]
      replaceRange(s.start, s.end, `@${m.name.trim() ? m.name.split('#')[0] : m.address} `)
    }
  }
  const insertAtCaret = (glyph: string): void => {
    const el = inputRef.current
    const start = el?.selectionStart ?? draft.length
    const end = el?.selectionEnd ?? draft.length
    replaceRange(start, end, glyph)
  }

  const send = (): void => {
    if (!draft.trim()) return
    chat.send(draft)
    setDraft('')
    setSug(null)
    setPicker(false)
  }

  const onKeyDown = (e: React.KeyboardEvent): void => {
    e.stopPropagation() // keep movement keys out of the engine while typing
    if (sug && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault()
      const n = sug.items.length
      setSug({ ...sug, sel: (sug.sel + (e.key === 'ArrowDown' ? 1 : n - 1)) % n })
    } else if (sug && (e.key === 'Enter' || e.key === 'Tab')) {
      e.preventDefault()
      accept(sug, sug.sel)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      send()
    } else if (isCancelKey(e)) {
      setSug(null)
      setPicker(false)
      inputRef.current?.blur()
    }
  }

  // The emoji panel closes on a press outside it (the emoji button toggles it itself).
  useEffect(() => {
    if (!picker) return
    const onDown = (e: PointerEvent): void => {
      const t = e.target as HTMLElement
      if (pickerRef.current?.contains(t) || t.closest('[aria-label="Emoji"]')) return
      setPicker(false)
    }
    document.addEventListener('pointerdown', onDown, true)
    return () => document.removeEventListener('pointerdown', onDown, true)
  }, [picker])

  // A click anywhere in the panel that isn't a control focuses the input — unless it selected text to copy.
  const focusFromPanel = (e: React.MouseEvent): void => {
    if (!open || (e.target as HTMLElement).closest('button, a, input, textarea, [role="button"], [role="dialog"]')) return
    if (window.getSelection()?.toString()) return
    inputRef.current?.focus()
  }

  const toggleEmoji = (): void => {
    openIfClosed()
    setPicker((p) => !p)
  }

  // Friends (and other left-docked panels) share the chat's bottom-left dock; hide the
  // chat entirely when one is open so they don't overlap.
  if (hidden) return null

  return (
    <div
      ref={hudInsetRef}
      className={`${styles.root} ${open ? styles.open : ''} ${active ? styles.active : ''} ${focused ? styles.focused : ''}`.trim()}
      onClick={focusFromPanel}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {open && (
        <header className={styles.nav}>
          <div className={styles.navLeft}>
            <DclLogo size={28} className={styles.channelIcon} />
            <span className={styles.navTitle}>Nearby</span>
          </div>
          <div className={styles.navRight}>
            <ControlButton
              variant="faint"
              shape="pill"
              className={styles.membersBtn}
              active={showMembers}
              aria-label={`${chat.members.length} nearby`}
              onClick={() => setShowMembers((s) => !s)}
            >
              <MaskIcon src={playersIcon} size={18} />
              {chat.members.length}
            </ControlButton>
            <span className={styles.navDivider} aria-hidden="true" />
            <ControlButton variant="dark" aria-label="Close chat" onClick={chat.toggle}>
              <MaskIcon src={closeIcon} size={10} />
            </ControlButton>
          </div>
        </header>
      )}

      {open && (
        <div ref={listRef} className={`${styles.messages} ${dim ? styles.dim : ''}`.trim()}>
          {rows.length === 0 ? (
            <div className={styles.empty}>No messages yet</div>
          ) : (
            rows.map((r) =>
              r.kind === 'day' ? (
                <DaySeparator key={r.id} ts={r.ts} />
              ) : (
                <Fragment key={r.line.id}>
                  {r.line.id === newFrom && <NewSeparator />}
                  <ChatBubble
                    line={r.line}
                    arrive={r.line.id >= liveFrom.current}
                    members={chat.members}
                    me={me}
                    onOpenProfile={openProfile}
                    onLocation={teleport}
                    onVisitWorld={hasVisitWorld ? visitWorld : undefined}
                  />
                </Fragment>
              )
            )
          )}
        </div>
      )}

      {open && (
        <button
          type="button"
          className={`${styles.toBottom} ${active && unread > 0 ? styles.toBottomShown : active ? styles.toBottomLeaving : ''}`.trim()}
          aria-label={`${unread} new messages`}
          tabIndex={active && unread > 0 ? 0 : -1}
          onClick={scrollToBottom}
        >
          {shownUnread.current > 9 ? '+9' : shownUnread.current}
        </button>
      )}

      {active && picker && (
        <div ref={pickerRef} className={styles.pickerWrap}>
          <EmojiPicker onPick={insertAtCaret} />
        </div>
      )}

      {active && sug && (
        <div className={`${styles.suggest} ${sug.kind === 'emoji' ? styles.suggestEmoji : styles.suggestPeople}`}>
          <ul className={styles.suggestList} role="listbox">
            {sug.kind === 'emoji'
              ? sug.items.map((e, i) => (
                  <li key={e.code} role="option" aria-selected={i === sug.sel}>
                    <button
                      type="button"
                      className={`${styles.suggestItem} ${i === sug.sel ? styles.suggestActive : ''}`.trim()}
                      onMouseDown={(ev) => ev.preventDefault()}
                      onClick={() => accept(sug, i)}
                    >
                      <span className={styles.suggestGlyph}>{e.emoji}</span>
                      <span className={styles.suggestName}>{e.expression}</span>
                    </button>
                  </li>
                ))
              : sug.items.map((m, i) => (
                  <li key={m.address} role="option" aria-selected={i === sug.sel}>
                    <button
                      type="button"
                      className={`${styles.suggestItem} ${i === sug.sel ? styles.suggestActive : ''}`.trim()}
                      onMouseDown={(ev) => ev.preventDefault()}
                      onClick={() => accept(sug, i)}
                    >
                      <Avatar src={m.picture} name={m.name} color={senderColor(m.address, m.name)} size={32} framed className={styles.avatar} />
                      <MemberName member={m} />
                    </button>
                  </li>
                ))}
          </ul>
        </div>
      )}

      <form
        className={styles.inputRow}
        onSubmit={(e) => {
          e.preventDefault()
          send()
        }}
      >
        <textarea
          ref={inputRef}
          rows={1}
          className={styles.input}
          value={draft}
          onChange={(e) => updateDraft(e.target.value, e.target.selectionStart ?? e.target.value.length)}
          onSelect={(e) => {
            const el = e.currentTarget
            if (el.selectionStart === el.selectionEnd && sug?.end !== el.selectionStart) suggestAt(el.value, el.selectionStart ?? el.value.length)
          }}
          onFocus={() => {
            setFocused(true)
            openIfClosed()
          }}
          onBlur={() => setFocused(false)}
          placeholder={focused ? 'Write a message' : 'Press Enter to chat'}
          maxLength={MAX_LEN}
          onKeyDown={onKeyDown}
        />
        {focused && draft.length > 0 && <CharRing len={draft.length} />}
        {!bare && (
          <button
            type="button"
            className={`${styles.emojiBtn} ${picker ? styles.emojiOn : ''}`.trim()}
            aria-label="Emoji"
            aria-pressed={picker}
            onMouseDown={(e) => e.preventDefault()}
            onClick={toggleEmoji}
          />
        )}
      </form>

      {open && active && showMembers && (
        <MembersOverlay
          members={chat.members}
          speaking={chat.speaking}
          onBack={() => setShowMembers(false)}
          onClose={() => {
            setShowMembers(false)
            chat.toggle()
          }}
        />
      )}
    </div>
  )
}

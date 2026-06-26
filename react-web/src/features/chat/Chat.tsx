// React DOM chat — Explorer 2.0 design, three states:
//   • collapsed (hidden): a borderless translucent input bar; focusing it opens chat
//   • open + idle (not hovered/focused): translucent — bubbles float over the world
//   • open + active (hover/focus): full solid panel — navbar, emoji, members, borders
// Incoming messages come from the bridge getChatStream relay; sends go via BevyApi.sendChat.

import { useEffect, useMemo, useRef, useState } from 'react'
import type { ChatLine, ChatState } from '../session/useEngineSession'
import type { NearbyMember } from '../../engine/protocol'
import { Avatar, ControlButton, DclLogo } from '../../design'
import { EmojiPicker } from './EmojiPicker'
import { searchByShortcode, SHORTCODE_RE, type Emoji } from './emojiData'
import { MessageText, mentionsMe, buildNameIndex } from './chatText'
import { ProfileCard, type ChatUser } from './ProfileCard'
import styles from './Chat.module.css'

const MAX_LEN = 500
const ADDRESS_RE = /^0x[0-9a-fA-F]{6,}$/

// DCL rarity name colors — gives each sender a stable, on-brand color.
const RARITY = [
  '#73d3d3', '#acf8f8', '#ff8362', '#ff4bed', '#caff73', '#a14bf3',
  '#e8b9ff', '#fea217', '#81e1ff', '#ff7439', '#ffa25a', '#ffc95b',
  '#a0abff', '#c640cd'
]
const SYSTEM_COLOR = '#61d04f'

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

function hash(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0
  return h
}

function senderColor(sender: string): string {
  if (isSystem(sender)) return SYSTEM_COLOR
  return RARITY[hash(sender) % RARITY.length]
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
}

function dayKey(ts: number): string {
  const d = new Date(ts)
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`
}

function formatDay(ts: number): string {
  if (dayKey(ts) === dayKey(Date.now())) return 'Today'
  return new Date(ts).toLocaleDateString([], {
    weekday: 'short',
    day: 'numeric',
    month: 'short'
  })
}

function PersonIcon(): React.JSX.Element {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="8" cy="5" r="2.6" stroke="currentColor" strokeWidth="1.4" />
      <path
        d="M3.2 13c0-2.4 2.1-3.8 4.8-3.8s4.8 1.4 4.8 3.8"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
      />
    </svg>
  )
}

function Smiley(): React.JSX.Element {
  return (
    <svg width="20" height="20" viewBox="0 0 22 22" fill="none" aria-hidden="true">
      <circle cx="11" cy="11" r="10" stroke="currentColor" strokeWidth="2" />
      <circle cx="7.6" cy="9" r="1.15" fill="currentColor" />
      <circle cx="14.4" cy="9" r="1.15" fill="currentColor" />
      <path
        d="M7 13.4c1 1.6 2.5 2.4 4 2.4s3-.8 4-2.4"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
      />
    </svg>
  )
}

function CharRing({ len }: { len: number }): React.JSX.Element {
  const pct = Math.min(1, len / MAX_LEN)
  const r = 8
  const circ = 2 * Math.PI * r
  const color = pct >= 0.9 ? 'var(--brand)' : pct >= 0.7 ? 'var(--gold)' : 'var(--green)'
  return (
    <svg className={styles.ring} width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">
      <circle cx="10" cy="10" r={r} fill="none" stroke="rgba(255,255,255,0.25)" strokeWidth="2" />
      <circle
        cx="10"
        cy="10"
        r={r}
        fill="none"
        stroke={color}
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={circ}
        strokeDashoffset={circ * (1 - pct)}
        transform="rotate(-90 10 10)"
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

const MSG_STYLES = { url: styles.url, mention: styles.mention, location: styles.location }

export function ChatBubble({
  line,
  name,
  picture,
  members = [],
  me,
  onOpenProfile,
  onLocation
}: {
  line: ChatLine
  name: string
  picture?: string
  members?: NearbyMember[]
  me?: { address?: string; name?: string } | null
  /** Open the profile viewer for a user, anchored at the click. */
  onOpenProfile?: (user: ChatUser, e: React.MouseEvent) => void
  /** A location link (x,y) in the message was clicked → teleport. */
  onLocation?: (x: number, y: number) => void
}): React.JSX.Element {
  const color = senderColor(line.sender)
  const { base, tag } = splitName(name)
  const sender: ChatUser = { address: line.sender, name, picture }
  const highlight = mentionsMe(line.message, me ?? null, buildNameIndex(members))

  // Open the profile menu — on left-click OR right-click (suppress the browser menu).
  const openSender = (e: React.MouseEvent): void => {
    if (e.type === 'contextmenu') e.preventDefault()
    onOpenProfile?.(sender, e)
  }
  // Clicking an @mention opens that user's profile (resolved against the roster).
  const onMention = (address: string, mname: string, e: React.MouseEvent): void => {
    if (e.type === 'contextmenu') e.preventDefault()
    const m = members.find((mm) => mm.address.toLowerCase() === address.toLowerCase())
    onOpenProfile?.({ address, name: m?.name ?? `@${mname}`, picture: m?.picture }, e)
  }

  return (
    <div className={`${styles.entry} ${highlight ? styles.mentionMe : ''}`.trim()}>
      <button type="button" className={styles.avatarBtn} aria-label={`View ${base}`} onClick={openSender} onContextMenu={openSender}>
        <Avatar src={picture} name={name} color={color} size={28} />
      </button>
      <div className={styles.bubble}>
        <button type="button" className={styles.name} style={{ color }} onClick={openSender} onContextMenu={openSender}>
          {base}
          {tag && <span className={styles.tag}>{tag}</span>}
        </button>
        <span className={styles.text}>
          <MessageText text={line.message} members={members} styles={MSG_STYLES} onMention={onMention} onLocation={(x, y) => onLocation?.(x, y)} />
        </span>
        <span className={styles.time}>{formatTime(line.ts)}</span>
      </div>
    </div>
  )
}

export function MemberRow({ member }: { member: NearbyMember }): React.JSX.Element {
  const { base, tag } = splitName(memberLabel(member))
  const color = senderColor(member.address)
  return (
    <div className={styles.memberRow}>
      <Avatar src={member.picture} name={base} color={color} size={40} status="online" />
      <div className={styles.memberInfo}>
        <span className={styles.memberName} style={{ color }}>
          {base}
          {tag && <span className={styles.tag}>{tag}</span>}
        </span>
        <span className={styles.memberStatus}>Online</span>
      </div>
    </div>
  )
}

function MembersOverlay({
  members,
  onBack,
  onClose
}: {
  members: NearbyMember[]
  onBack: () => void
  onClose: () => void
}): React.JSX.Element {
  return (
    <div className={styles.membersPanel}>
      <header className={styles.membersHeader}>
        <ControlButton variant="ghost" className={styles.glyphLg} aria-label="Back" onClick={onBack}>
          ‹
        </ControlButton>
        <span className={styles.membersTitle}>Nearby</span>
        <span className={styles.membersCount}>
          <span className={styles.personIcon} aria-hidden="true">
            ●
          </span>
          {members.length} Online
        </span>
        <ControlButton variant="solid" className={styles.glyph} aria-label="Close chat" onClick={onClose}>
          ×
        </ControlButton>
      </header>
      <div className={styles.membersList}>
        {members.length === 0 ? (
          <div className={styles.empty}>No one nearby</div>
        ) : (
          members.map((m) => <MemberRow key={m.address} member={m} />)
        )}
      </div>
    </div>
  )
}

export function Chat({
  chat,
  hidden = false,
  me,
  onAddFriend,
  onBlock,
  onViewProfile,
  onTeleport
}: {
  chat: ChatState
  hidden?: boolean
  /** The local player (for @-me highlight + hiding self-actions in the viewer). */
  me?: { address?: string; name?: string } | null
  /** Add-friend from the profile viewer. */
  onAddFriend?: (address: string) => void
  /** Block from the profile viewer. */
  onBlock?: (address: string) => void
  /** Open the full passport for a user (View Profile). */
  onViewProfile?: (user: ChatUser) => void
  /** A location link (x,y) in a message was clicked. */
  onTeleport?: (x: number, y: number) => void
}): React.JSX.Element | null {
  const [draft, setDraft] = useState('')
  const [picker, setPicker] = useState(false)
  const [showMembers, setShowMembers] = useState(false)
  const [suggestions, setSuggestions] = useState<Emoji[]>([])
  const [scQuery, setScQuery] = useState<string | null>(null)
  const [hovered, setHovered] = useState(false)
  const [focused, setFocused] = useState(false)
  const [viewUser, setViewUser] = useState<{ user: ChatUser; x: number; y: number } | null>(null)
  const [mentionQuery, setMentionQuery] = useState<string | null>(null)
  const [mentionSug, setMentionSug] = useState<NearbyMember[]>([])
  const listRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const open = chat.open
  // "active" = the user is interacting → show the full solid panel + chrome.
  const active = open && (hovered || focused || picker)
  const bare = !active // collapsed or idle-open → borderless translucent input only

  const nameByAddr = useMemo(() => {
    const m = new Map<string, string>()
    for (const mem of chat.members) if (mem.name.trim()) m.set(mem.address.toLowerCase(), mem.name)
    return m
  }, [chat.members])
  const pictureByAddr = useMemo(() => {
    const m = new Map<string, string>()
    for (const mem of chat.members) if (mem.picture) m.set(mem.address.toLowerCase(), mem.picture)
    return m
  }, [chat.members])
  const resolveName = (sender: string): string =>
    nameByAddr.get(sender.toLowerCase()) ?? displaySender(sender)

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

  useEffect(() => {
    if (!open) return
    const el = listRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [chat.messages, open, active])

  // Opening chat (e.g. via the sidebar icon) focuses the input so it comes up in the
  // active/focused state, ready to type — matches Unity's "click chat → start typing".
  useEffect(() => {
    if (open) inputRef.current?.focus()
  }, [open])

  // Leaving the chat (click outside → not hovered/focused) resets the nearby-members
  // overlay, so re-entering shows messages — not the members list left open from before.
  useEffect(() => {
    if (!active) setShowMembers(false)
  }, [active])

  const openIfClosed = (): void => {
    if (!chat.open) chat.toggle()
  }

  // Profile viewer: clicking a name/avatar/@mention opens a mini passport at the click.
  const openProfile = (user: ChatUser, e: React.MouseEvent): void => {
    setViewUser({ user, x: e.clientX, y: e.clientY })
  }
  // "Mention" from the viewer drops @name into the draft, ready to send.
  const insertMention = (name: string): void => {
    setDraft((d) => `${d.replace(/\s*$/, '')} @${name} `.trimStart())
    openIfClosed()
    inputRef.current?.focus()
  }

  const MENTION_RE = /@([\w-]*)$/
  const updateDraft = (value: string): void => {
    setDraft(value)
    const m = value.match(SHORTCODE_RE)
    setScQuery(m ? m[1] : null)
    setSuggestions(m ? searchByShortcode(m[1]) : [])
    // @mention autocomplete from the nearby roster (trailing @partial).
    const mm = value.match(MENTION_RE)
    setMentionQuery(mm ? mm[1] : null)
    const q = (mm?.[1] ?? '').toLowerCase()
    setMentionSug(
      mm ? chat.members.filter((p) => (p.name || p.address).toLowerCase().includes(q)).slice(0, 6) : []
    )
  }

  const applyMention = (member: NearbyMember): void => {
    const label = member.name.trim() ? member.name.split('#')[0] : member.address
    setDraft((d) => d.replace(MENTION_RE, `@${label} `))
    setMentionSug([])
    setMentionQuery(null)
    inputRef.current?.focus()
  }

  const applyEmoji = (glyph: string): void => {
    setDraft((d) => {
      const m = d.match(SHORTCODE_RE)
      return (m ? d.slice(0, m.index) : d) + glyph
    })
    setSuggestions([])
    setScQuery(null)
    inputRef.current?.focus()
  }

  const send = (): void => {
    if (!draft.trim()) return
    chat.send(draft)
    setDraft('')
    setSuggestions([])
    setScQuery(null)
    setMentionSug([])
    setMentionQuery(null)
  }

  const onKeyDown = (e: React.KeyboardEvent): void => {
    e.stopPropagation() // keep movement keys out of the engine while typing
    if (e.key === 'Enter') {
      e.preventDefault()
      if (mentionSug.length > 0) applyMention(mentionSug[0])
      else if (suggestions.length > 0) applyEmoji(suggestions[0].emoji)
      else send()
    } else if (e.key === 'Escape') {
      if (mentionQuery != null) {
        setMentionQuery(null)
        setMentionSug([])
      } else if (scQuery != null) {
        setScQuery(null)
        setSuggestions([])
      } else if (picker) setPicker(false)
    }
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
      className={`${styles.root} ${open ? styles.open : ''} ${active ? styles.active : ''}`.trim()}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {open && active && (
        <header className={styles.nav}>
          <div className={styles.navLeft}>
            <DclLogo size={26} className={styles.channelIcon} />
            <span className={styles.navTitle}>Nearby</span>
          </div>
          <div className={styles.navRight}>
            {chat.members.length > 0 && (
              <ControlButton
                variant="ghost"
                shape="pill"
                active={showMembers}
                aria-label={`${chat.members.length} nearby`}
                onClick={() => setShowMembers((s) => !s)}
              >
                <PersonIcon />
                {chat.members.length}
              </ControlButton>
            )}
            <ControlButton variant="solid" className={styles.glyph} aria-label="Close chat" onClick={chat.toggle}>
              ×
            </ControlButton>
          </div>
        </header>
      )}

      {open && (
        <div ref={listRef} className={styles.messages}>
          {rows.length === 0 ? (
            <div className={styles.empty}>No messages yet</div>
          ) : (
            rows.map((r) =>
              r.kind === 'day' ? (
                <DaySeparator key={r.id} ts={r.ts} />
              ) : (
                <ChatBubble
                  key={r.line.id}
                  line={r.line}
                  name={resolveName(r.line.sender)}
                  picture={pictureByAddr.get(r.line.sender.toLowerCase())}
                  members={chat.members}
                  me={me}
                  onOpenProfile={openProfile}
                  onLocation={(x, y) => onTeleport?.(x, y)}
                />
              )
            )
          )}
        </div>
      )}

      {active && picker && (
        <div className={styles.pickerWrap}>
          <EmojiPicker onPick={applyEmoji} onClose={() => setPicker(false)} />
        </div>
      )}

      {active && mentionQuery != null && mentionSug.length > 0 && (
        <div className={styles.suggest}>
          <ul className={styles.suggestList}>
            {mentionSug.map((m, i) => (
              <li key={m.address}>
                <button
                  type="button"
                  className={`${styles.suggestItem} ${i === 0 ? styles.suggestActive : ''}`.trim()}
                  onClick={() => applyMention(m)}
                >
                  <Avatar src={m.picture} name={m.name} color={senderColor(m.address)} size={20} />
                  <span className={styles.suggestName}>{m.name || shortAddr(m.address)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {active && scQuery != null && (
        <div className={styles.suggest}>
          {suggestions.length === 0 ? (
            <div className={styles.noResults}>No results</div>
          ) : (
            <ul className={styles.suggestList}>
              {suggestions.map((e, i) => (
                <li key={e.code}>
                  <button
                    type="button"
                    className={`${styles.suggestItem} ${i === 0 ? styles.suggestActive : ''}`.trim()}
                    onClick={() => applyEmoji(e.emoji)}
                  >
                    <span className={styles.suggestGlyph}>{e.emoji}</span>
                    <span className={styles.suggestName}>{e.expression}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <form
        className={styles.inputRow}
        onSubmit={(e) => {
          e.preventDefault()
          send()
        }}
      >
        <input
          ref={inputRef}
          className={`${styles.input} ${bare ? styles.inputBare : ''}`.trim()}
          value={draft}
          onChange={(e) => updateDraft(e.target.value)}
          onFocus={() => {
            setFocused(true)
            openIfClosed()
          }}
          onBlur={() => setFocused(false)}
          placeholder={focused ? 'Message Nearby' : 'Press Enter to chat'}
          maxLength={MAX_LEN}
          onKeyDown={onKeyDown}
        />
        {!bare && draft.length > 0 && <CharRing len={draft.length} />}
        {!bare && (
          <ControlButton
            variant="ghost"
            size="sm"
            active={picker}
            className={styles.emojiBtn}
            aria-label="Emoji"
            onClick={toggleEmoji}
          >
            <Smiley />
          </ControlButton>
        )}
      </form>

      {open && active && showMembers && (
        <MembersOverlay
          members={chat.members}
          onBack={() => setShowMembers(false)}
          onClose={() => {
            setShowMembers(false)
            chat.toggle()
          }}
        />
      )}

      {viewUser && (
        <ProfileCard
          user={viewUser.user}
          x={viewUser.x}
          y={viewUser.y}
          me={me}
          onAddFriend={onAddFriend}
          onBlock={onBlock}
          onViewProfile={onViewProfile}
          onMention={insertMention}
          onClose={() => setViewUser(null)}
        />
      )}
    </div>
  )
}

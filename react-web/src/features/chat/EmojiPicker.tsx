// Emoji picker — eight section toggles, a search field, and every
// section in one scroll. Clicking an emoji inserts its glyph and keeps the panel open.

import { useMemo, useRef, useState } from 'react'
import { MaskIcon, SearchField } from '../../design'
import { EMOJI_BY_CODE, EMOJI_GROUPS, loadRecents, pushRecent, searchByShortcode, type Emoji } from './emojiData'
import smileysIcon from '../../assets/chat/emoji-categories/smileyspeople.png'
import animalsIcon from '../../assets/chat/emoji-categories/animals.png'
import foodIcon from '../../assets/chat/emoji-categories/food.png'
import activitiesIcon from '../../assets/chat/emoji-categories/activities.png'
import placesIcon from '../../assets/chat/emoji-categories/places.png'
import objectsIcon from '../../assets/chat/emoji-categories/objects.png'
import symbolsIcon from '../../assets/chat/emoji-categories/symbols.png'
import flagIcon from '../../assets/chat/emoji-categories/flag.png'
import { isCancelKey } from '../../lib/bindingLabels'
import styles from './EmojiPicker.module.css'

// Toggle order differs from the scroll order: smileys + people share one, activities comes before places.
const TOGGLES: { icon: string; label: string; section: string }[] = [
  { icon: smileysIcon, label: 'Smileys and people', section: 'Smileys & Emotion' },
  { icon: animalsIcon, label: 'Animals and nature', section: 'Animals & Nature' },
  { icon: foodIcon, label: 'Food and drink', section: 'Food & Drink' },
  { icon: activitiesIcon, label: 'Activities', section: 'Activities' },
  { icon: placesIcon, label: 'Travel and places', section: 'Travel & Places' },
  { icon: objectsIcon, label: 'Objects', section: 'Objects' },
  { icon: symbolsIcon, label: 'Symbols', section: 'Symbols' },
  { icon: flagIcon, label: 'Flags', section: 'Flags' }
]

function Grid({ emojis, onPick }: { emojis: Emoji[]; onPick: (e: Emoji) => void }): React.JSX.Element {
  return (
    <div className={styles.grid}>
      {emojis.map((e) => (
        <button key={e.code} type="button" className={styles.emoji} title={e.expression} onClick={() => onPick(e)}>
          {e.emoji}
        </button>
      ))}
    </div>
  )
}

export function EmojiPicker({ onPick, onClose }: { onPick: (glyph: string) => void; onClose?: () => void }): React.JSX.Element {
  const [query, setQuery] = useState('')
  const [section, setSection] = useState<string | null>(null)
  const bodyRef = useRef<HTMLDivElement>(null)
  const [recents, setRecents] = useState<string[]>(() => loadRecents())
  const q = query.trim()
  const results = useMemo(() => (q ? searchByShortcode(q, 200) : []), [q])
  const recentEmojis = useMemo(() => recents.map((c) => EMOJI_BY_CODE.get(c)).filter((e): e is Emoji => e != null), [recents])
  const pick = (e: Emoji): void => {
    onPick(e.emoji)
    setRecents(pushRecent(e.code))
  }

  const jump = (name: string): void => {
    setQuery('')
    setSection(name)
    requestAnimationFrame(() => {
      const el = bodyRef.current?.querySelector<HTMLElement>(`[data-section="${name}"]`)
      if (el && bodyRef.current) bodyRef.current.scrollTop = el.offsetTop
    })
  }

  // The toggle follows the section at the top of the scroll.
  const onScroll = (): void => {
    const body = bodyRef.current
    if (!body || q) return
    let current = section ?? TOGGLES[0].section
    for (const el of body.querySelectorAll<HTMLElement>('[data-section]')) {
      if (el.offsetTop <= body.scrollTop + 1) current = el.dataset.section ?? current
    }
    if (section == null && body.scrollTop === 0) return
    const toggle = current === 'People & Body' || current === 'Frequently used' ? 'Smileys & Emotion' : current
    if (toggle !== section) setSection(toggle)
  }

  return (
    <div className={styles.root} role="dialog" aria-label="Emoji picker" onKeyDown={(e) => {
        e.stopPropagation()
        if (isCancelKey(e)) onClose?.()
      }}
    >
      <div className={styles.tabs}>
        {TOGGLES.map((t) => (
          <button
            key={t.section}
            type="button"
            className={`${styles.tab} ${t.section === section && !q ? styles.tabActive : ''}`.trim()}
            aria-label={t.label}
            aria-pressed={t.section === section && !q}
            onClick={() => jump(t.section)}
          >
            <MaskIcon src={t.icon} size={25} />
          </button>
        ))}
      </div>

      <div className={styles.searchRow}>
        <SearchField variant="light" size="sm" value={query} onChange={setQuery} placeholder="Search emoji" />
      </div>

      <div ref={bodyRef} className={styles.body} onScroll={onScroll}>
        {q ? (
          results.length > 0 ? (
            <Grid emojis={results} onPick={pick} />
          ) : (
            <div className={styles.noResults}>No results</div>
          )
        ) : (
          <>
            {recentEmojis.length > 0 && (
              <section className={styles.section} data-section="Frequently used">
                <div className={styles.sectionHeader}>Frequently used</div>
                <Grid emojis={recentEmojis} onPick={pick} />
              </section>
            )}
            {EMOJI_GROUPS.map((g) => (
              <section key={g.name} className={styles.section} data-section={g.name}>
                <div className={styles.sectionHeader}>{g.name}</div>
                <Grid emojis={g.emojis} onPick={pick} />
              </section>
            ))}
          </>
        )}
      </div>
    </div>
  )
}

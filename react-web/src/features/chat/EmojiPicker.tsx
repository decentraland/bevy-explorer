// Emoji picker — category tabs + search + "Frequently used" + per-category grid,
// matching the Explorer 2.0 emoji panel. Clicking inserts the Unicode glyph and
// records it in recents. Ported/expanded from the SDK7 scene's emoji button.

import { useMemo, useState } from 'react'
import {
  EMOJI_BY_CODE,
  EMOJI_GROUPS,
  loadRecents,
  pushRecent,
  searchByShortcode,
  type Emoji
} from './emojiData'
import styles from './EmojiPicker.module.css'

function Grid({
  emojis,
  onPick
}: {
  emojis: Emoji[]
  onPick: (e: Emoji) => void
}): React.JSX.Element {
  return (
    <div className={styles.grid}>
      {emojis.map((e) => (
        <button
          key={e.code}
          type="button"
          className={styles.emoji}
          title={e.expression}
          onClick={() => onPick(e)}
        >
          {e.emoji}
        </button>
      ))}
    </div>
  )
}

export function EmojiPicker({
  onPick,
  onClose
}: {
  onPick: (glyph: string) => void
  onClose: () => void
}): React.JSX.Element {
  const [active, setActive] = useState(0)
  const [query, setQuery] = useState('')
  const [recents, setRecents] = useState<string[]>(() => loadRecents())

  const pick = (e: Emoji): void => {
    onPick(e.emoji)
    setRecents(pushRecent(e.code))
  }

  const q = query.trim()
  const results = useMemo(() => (q ? searchByShortcode(q, 80) : []), [q])
  const recentEmojis = useMemo(
    () => recents.map((c) => EMOJI_BY_CODE.get(c)).filter((e): e is Emoji => e != null),
    [recents]
  )
  const group = EMOJI_GROUPS[active]

  return (
    <div className={styles.root} role="dialog" aria-label="Emoji picker">
      <div className={styles.tabs}>
        {EMOJI_GROUPS.map((g, i) => (
          <button
            key={g.name}
            type="button"
            className={`${styles.tab} ${i === active && !q ? styles.tabActive : ''}`.trim()}
            title={g.name}
            onClick={() => {
              setActive(i)
              setQuery('')
            }}
          >
            {g.icon}
          </button>
        ))}
        <button type="button" className={styles.close} title="Close" onClick={onClose}>
          ×
        </button>
      </div>

      <div className={styles.searchRow}>
        <span className={styles.searchIcon} aria-hidden="true">
          🔍
        </span>
        <input
          className={styles.search}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search emoji"
          onKeyDown={(e) => e.stopPropagation()}
        />
      </div>

      <div className={styles.body}>
        {q ? (
          results.length > 0 ? (
            <Grid emojis={results} onPick={pick} />
          ) : (
            <div className={styles.noResults}>No results</div>
          )
        ) : (
          <>
            {recentEmojis.length > 0 && (
              <>
                <div className={styles.sectionHeader}>Frequently used</div>
                <Grid emojis={recentEmojis} onPick={pick} />
              </>
            )}
            <div className={styles.sectionHeader}>{group.name}</div>
            <Grid emojis={group.emojis} onPick={pick} />
          </>
        )}
      </div>
    </div>
  )
}

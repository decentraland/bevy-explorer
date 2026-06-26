// Emotes wheel — 10 EmoteSlot cards arranged radially (the card shape tilts outward,
// its silhouette + number stay upright). Centre shows the hovered emote + hints. Click a
// card to play it. Built from the Figma-matched EmoteSlot (node 10386-4701).

import { useState } from 'react'
import { catalystThumbUrl } from '../../lib/identity'
import { EmoteSlot } from './EmoteSlot'
import type { EmotesState } from '../session/useEngineSession'
import styles from './EmotesWheel.module.css'

const CENTER = 215 // wheel viewport is 430×430 (then HUD-scaled)
const RADIUS = 152 // centre → slot centre
const CARD_W = 112

export function EmotesWheel({ emotes }: { emotes: EmotesState }): React.JSX.Element | null {
  const [hover, setHover] = useState<number | null>(null)
  if (!emotes.open) return null

  // number 1 at top (−90°), clockwise; 0 sits just left of 1.
  const slots = Array.from({ length: 10 }, (_, k) => {
    const num = (k + 1) % 10
    return { k, num, deg: -90 + k * 36, emote: emotes.list.find((e) => e.slot === num) ?? null }
  })
  const hovered = hover != null ? emotes.list.find((e) => e.slot === hover) : null

  return (
    <div className={styles.backdrop} onClick={emotes.toggle}>
      <div className={styles.wheel} onClick={(e) => e.stopPropagation()}>
        <button type="button" className={styles.close} aria-label="Close emotes" onClick={emotes.toggle}>
          ×
        </button>

        {slots.map(({ num, deg, emote }) => {
          const rad = (deg * Math.PI) / 180
          const cx = CENTER + RADIUS * Math.cos(rad)
          const cy = CENTER + RADIUS * Math.sin(rad)
          return (
            <div
              key={num}
              className={`${styles.slotPos} ${hover === num ? styles.slotHover : ''} ${emote ? '' : styles.slotEmpty}`.trim()}
              style={{ left: `${cx}px`, top: `${cy}px` }}
              onMouseEnter={() => emote && setHover(num)}
              onMouseLeave={() => setHover((h) => (h === num ? null : h))}
              onClick={() => emote && emotes.play(emote.urn)}
            >
              <EmoteSlot
                thumbnail={emote ? catalystThumbUrl(emote.urn) : undefined}
                rarity={emote?.rarity ?? 'base'}
                number={num}
                rotate={deg + 90}
                width={CARD_W}
              />
            </div>
          )
        })}

        <div className={styles.center}>
          <div className={styles.hoverName}>{hovered?.name ?? ' '}</div>
          <div className={styles.title}>EMOTES</div>
          <div className={styles.customise}>Customise [E]</div>
          <div className={styles.hint}>
            Hold [B+num] to run an emote
            <br />
            while the wheel is closed
          </div>
        </div>
      </div>
    </div>
  )
}

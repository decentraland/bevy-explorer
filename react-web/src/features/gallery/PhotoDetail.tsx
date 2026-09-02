// Full-screen photo viewer (lightbox) for one camera-reel photo: large image with
// prev/next navigation, an info sidebar (date · place · owner · people in the shot),
// and the reel actions (jump in · download · copy link · share to X · delete).
//
// Rendered INSIDE MainMenuShell's scaled reference-canvas body, so it's DPI-correct
// without any portal/--ui-scale gymnastics. While mounted it registers as a cancel layer,
// so the engine's Cancel action closes the lightbox first (key or gamepad), leaving the
// next press to close the gallery.

import { useEffect, useRef, useState } from 'react'
import { serviceUrl } from '../../lib/baseDomain'
import { Avatar, Button } from '../../design'
import { useWindowKeyDown } from '../../lib/useWindowKeyDown'
import { registerCancelLayer } from '../../lib/cancelLayers'
import { photoTime } from '../session/useEngineSession'
import type { GalleryPhoto, GalleryPhotoMeta } from '../../engine/protocol'
import type { ChatUser } from '../chat/ProfileCardPresentation'
import styles from './GalleryPage.module.css'

const REELS_BASE = serviceUrl('reels')

function formatDate(dateTime: string): string {
  const ms = photoTime(dateTime)
  if (!ms) return ''
  return new Date(ms).toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' })
}

// --- small action glyphs (feature-specific) ----------------------------------
const Glyph = ({ d }: { d: string }): React.JSX.Element => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d={d} />
  </svg>
)
const ICON = {
  jump: 'M10 17l5-5-5-5v10z',
  download: 'M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z',
  link: 'M3.9 12a3.1 3.1 0 013.1-3.1h4V7H7a5 5 0 100 10h4v-1.9H7A3.1 3.1 0 013.9 12zM17 7h-4v1.9h4a3.1 3.1 0 010 6.2h-4V17h4a5 5 0 100-10zM8 11h8v2H8z',
  share: 'M18 16.08c-.76 0-1.44.3-1.96.77L8.91 12.7c.05-.23.09-.46.09-.7s-.04-.47-.09-.7l7.05-4.11c.54.5 1.25.81 2.04.81a3 3 0 100-6 3 3 0 00-3 3c0 .24.04.47.09.7L8.04 9.81A3 3 0 003 12a3 3 0 003 3 3 3 0 001.96-.73l7.12 4.16c-.05.21-.08.43-.08.65a2.92 2.92 0 105.84 0 3 3 0 00-2.84-2.65z',
  trash: 'M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z'
} as const

export function PhotoDetail({
  photos,
  index,
  meta,
  isSelf,
  onIndex,
  onLoadMeta,
  onClose,
  onTeleport,
  onDelete,
  onViewPerson
}: {
  photos: GalleryPhoto[]
  index: number
  meta: GalleryPhotoMeta | null | undefined
  /** The gallery is the local player's own, so delete is allowed. */
  isSelf: boolean
  onIndex: (i: number) => void
  onLoadMeta: (id: string) => void
  onClose: () => void
  onTeleport: (x: number, y: number) => void
  onDelete?: (id: string) => void
  onViewPerson?: (user: ChatUser) => void
}): React.JSX.Element {
  const photo = photos[index]
  const [copied, setCopied] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const hasPrev = index > 0
  const hasNext = index < photos.length - 1

  // Lazily fetch this photo's metadata the first time it's viewed.
  useEffect(() => {
    if (meta === undefined) onLoadMeta(photo.id)
  }, [photo.id, meta, onLoadMeta])

  // Reset transient state when switching photos.
  useEffect(() => {
    setCopied(false)
    setConfirmDelete(false)
  }, [photo.id])

  // The engine's Cancel action closes the lightbox before the session closes the gallery —
  // registered as a cancel layer for the lightbox's lifetime (ref: onClose is an inline
  // prop, and re-registering per render would churn the layer stack).
  const onCloseRef = useRef(onClose)
  onCloseRef.current = onClose
  useEffect(() => registerCancelLayer(() => onCloseRef.current()), [])

  // Keyboard: arrows navigate.
  useWindowKeyDown((e) => {
    if (e.key === 'ArrowLeft' && hasPrev) {
      onIndex(index - 1)
    } else if (e.key === 'ArrowRight' && hasNext) {
      onIndex(index + 1)
    }
  })

  const copyLink = (): void => {
    navigator.clipboard
      ?.writeText(`${REELS_BASE}/${photo.id}`)
      .then(() => setCopied(true))
      .catch(() => console.warn('[gallery] clipboard write failed'))
  }
  const shareToX = (): void => {
    const text = encodeURIComponent('Check out my photo from Decentraland 👋')
    const url = encodeURIComponent(`${REELS_BASE}/${photo.id}`)
    window.open(`https://x.com/intent/tweet?text=${text}&url=${url}`, '_blank', 'noopener')
  }

  const date = formatDate(photo.dateTime)
  const place = meta?.sceneName
  const hasCoords = meta?.x != null && meta?.y != null
  const people = meta?.people ?? []

  return (
    <div className={styles.viewer} role="dialog" aria-modal="true" aria-label="Photo">
      <button type="button" className={styles.viewerClose} aria-label="Close" onClick={onClose}>
        ×
      </button>

      <div className={styles.stage}>
        <button
          type="button"
          className={`${styles.nav} ${styles.navPrev}`}
          aria-label="Previous photo"
          disabled={!hasPrev}
          onClick={() => hasPrev && onIndex(index - 1)}
        >
          ‹
        </button>
        <img className={styles.stageImg} src={photo.url} alt="" />
        <button
          type="button"
          className={`${styles.nav} ${styles.navNext}`}
          aria-label="Next photo"
          disabled={!hasNext}
          onClick={() => hasNext && onIndex(index + 1)}
        >
          ›
        </button>
      </div>

      <aside className={styles.info}>
        <div className={styles.actions}>
          {hasCoords && (
            <Button size="sm" variant="ghost" onClick={() => onTeleport(meta!.x!, meta!.y!)}>
              <Glyph d={ICON.jump} /> Jump In
            </Button>
          )}
          <a className={styles.actionLink} href={photo.url} target="_blank" rel="noopener noreferrer" download>
            <Glyph d={ICON.download} /> Download
          </a>
          <Button size="sm" variant="ghost" onClick={copyLink}>
            <Glyph d={ICON.link} /> {copied ? 'Copied!' : 'Copy link'}
          </Button>
          <Button size="sm" variant="ghost" onClick={shareToX}>
            <Glyph d={ICON.share} /> Share
          </Button>
          {isSelf && onDelete && (
            <Button
              size="sm"
              variant="ghost"
              className={styles.deleteBtn}
              onClick={() => (confirmDelete ? onDelete(photo.id) : setConfirmDelete(true))}
            >
              <Glyph d={ICON.trash} /> {confirmDelete ? 'Confirm?' : 'Delete'}
            </Button>
          )}
        </div>

        {date && (
          <div className={styles.infoBlock}>
            <span className={styles.infoLabel}>Date</span>
            <span className={styles.infoValue}>{date}</span>
          </div>
        )}

        {place && (
          <div className={styles.infoBlock}>
            <span className={styles.infoLabel}>Scene</span>
            <span className={styles.infoValue}>
              {place}
              {hasCoords && <span className={styles.coords}> · {meta!.x}, {meta!.y}</span>}
            </span>
          </div>
        )}

        {people.length > 0 && (
          <div className={styles.infoBlock}>
            <span className={styles.infoLabel}>People in this photo</span>
            <div className={styles.people}>
              {people.map((person) => (
                <button
                  key={person.address}
                  type="button"
                  className={styles.person}
                  onClick={() => onViewPerson?.({ address: person.address, name: person.name })}
                >
                  <Avatar name={person.name || person.address} size={28} />
                  <span className={styles.personName}>{person.name || `${person.address.slice(0, 6)}…`}</span>
                </button>
              ))}
            </div>
          </div>
        )}
      </aside>
    </div>
  )
}

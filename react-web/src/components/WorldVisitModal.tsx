// Shared "jump to this world?" confirmation, mirroring unity-explorer's ChangeRealmPrompt.
// Triggered from the map (world search results) and from chat (a `*.dcl.eth` link). Given just the
// world name it looks the world up in the places API for the friendly title, description and cover
// image. Confirm → onConfirm (the caller calls the engine's changeRealm via the bridge).

import { useEffect, useState } from 'react'
import { serviceUrl } from '../lib/baseDomain'
import { openPopup } from '../design'
import styles from './WorldVisitModal.module.css'

const WORLDS_API = `${serviceUrl('places')}/api/worlds`

/** The bits of a places-API world entry this prompt shows. All optional — a world may have none. */
interface WorldInfo {
  title?: string
  description?: string
  image?: string
}

export function WorldVisitModal({
  worldName,
  title,
  onCancel,
  onConfirm
}: {
  worldName: string
  /** Friendly title the caller already has (map search does) — shown instantly while the rest loads. */
  title?: string
  onCancel: () => void
  onConfirm: () => void
}): React.JSX.Element {
  const [info, setInfo] = useState<WorldInfo>({ title })
  // A world can advertise an image that 404s (unpinned content) — drop it rather than leave a blank box.
  const [imageBroken, setImageBroken] = useState(false)

  // Always look the world up: even when the caller passed a title, the description and cover image
  // come from here. Seeded with `title` above, so the name shows immediately and the rest fills in.
  useEffect(() => {
    const ac = new AbortController()
    setImageBroken(false) // a different world gets a fresh shot at its image
    fetch(`${WORLDS_API}?names=${encodeURIComponent(worldName)}`, { signal: ac.signal })
      .then((r) => r.json())
      .then((j: { data?: WorldInfo[] }) => {
        const w = j.data?.[0]
        if (w) setInfo((prev) => ({ title: w.title || prev.title, description: w.description, image: w.image }))
      })
      .catch(() => undefined) // offline / unknown world → name-only prompt, still usable
    return () => ac.abort()
  }, [worldName])

  const friendly = info.title && info.title !== worldName ? info.title : ''

  return (
    <div className={styles.modal} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Visit world">
      <button type="button" className={styles.close} aria-label="Close" onClick={onCancel}>✕</button>
      <div className={styles.question}>Do you want to jump to the following world?</div>
      {info.image && !imageBroken && (
        // Decorative: the world name/title right below already names the destination.
        <img className={styles.cover} src={info.image} alt="" loading="lazy" onError={() => setImageBroken(true)} />
      )}
      <div className={styles.realm}>{worldName}</div>
      {friendly && <div className={styles.realmSub}>{friendly}</div>}
      {info.description && <p className={styles.description}>{info.description}</p>}
      <div className={styles.actions}>
        <button type="button" className={styles.cancel} onClick={onCancel}>CANCEL</button>
        <button type="button" className={styles.go} onClick={onConfirm}>CONTINUE</button>
      </div>
    </div>
  )
}

/** Open the world-jump confirmation as a popup; returns the close handle. The popup layer owns the
 *  scrim, DPI scale, entrance, focus trap and Escape — any dismiss path other than CONTINUE cancels. */
export function openWorldVisit(opts: {
  worldName: string
  title?: string
  onConfirm: () => void
}): () => void {
  return openPopup((close) => (
    <WorldVisitModal
      worldName={opts.worldName}
      title={opts.title}
      onCancel={close}
      onConfirm={() => {
        close()
        opts.onConfirm()
      }}
    />
  ))
}

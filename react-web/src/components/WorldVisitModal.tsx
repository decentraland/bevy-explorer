// Shared "jump to this realm?" confirmation, mirroring unity-explorer's ChangeRealmPrompt.
// Triggered from the map (world search results) and from chat (a `*.dcl.eth` link). Given just
// the world name, it looks up the friendly title for the subtitle. Confirm → onConfirm (the
// caller calls the engine's changeRealm via the bridge).

import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import styles from './WorldVisitModal.module.css'

const WORLDS_API = 'https://places.decentraland.org/api/worlds'

export function WorldVisitModal({
  worldName,
  title,
  onCancel,
  onConfirm
}: {
  worldName: string
  /** Friendly scene title, if the caller already has it (map search does); else looked up. */
  title?: string
  onCancel: () => void
  onConfirm: () => void
}): React.JSX.Element {
  const [friendly, setFriendly] = useState(title ?? '')

  useEffect(() => {
    if (title) return
    const ac = new AbortController()
    fetch(`${WORLDS_API}?names=${encodeURIComponent(worldName)}`, { signal: ac.signal })
      .then((r) => r.json())
      .then((j: { data?: { title?: string }[] }) => setFriendly(j.data?.[0]?.title ?? ''))
      .catch(() => undefined)
    return () => ac.abort()
  }, [worldName, title])

  return createPortal(
    <div className={styles.overlay} onClick={onCancel}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Visit world">
        <button type="button" className={styles.close} aria-label="Close" onClick={onCancel}>✕</button>
        <div className={styles.question}>Do you want to jump to the following realm?</div>
        <div className={styles.realm}>{worldName}</div>
        {friendly && friendly !== worldName && <div className={styles.realmSub}>{friendly}</div>}
        <div className={styles.actions}>
          <button type="button" className={styles.cancel} onClick={onCancel}>CANCEL</button>
          <button type="button" className={styles.go} onClick={onConfirm}>CONTINUE</button>
        </div>
      </div>
    </div>,
    document.body
  )
}

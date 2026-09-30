// React Gallery (camera reel) — a full-screen page inside the shared MainMenuShell,
// matching the Unity Camera Reel: a month-grouped grid of the local player's in-world
// photos with a storage indicator, opening a full-screen PhotoDetail lightbox on click.
// Data comes from the bridge relay of camera-reel-service (see bridge-scene/domains/gallery).

import { useMemo, useState } from 'react'
import { EmptyState, Icon, Spinner } from '../../design'
import { MainMenuShell } from '../menu/MainMenuShell'
import { PhotoDetail } from './PhotoDetail'
import { photoTime, type GalleryState, type ProfileState } from '../session/useEngineSession'
import type { GalleryPhoto } from '../../engine/protocol'
import type { ChatUser } from '../chat/ProfileCardPresentation'
import styles from './GalleryPage.module.css'

function monthLabel(ms: number): string {
  return new Date(ms).toLocaleDateString('en-US', { month: 'long', year: 'numeric' })
}

type Group = { key: string; items: { photo: GalleryPhoto; index: number }[] }

export function GalleryPage({
  gallery,
  profile,
  onNavigate,
  onTeleport,
  onViewProfile
}: {
  gallery: GalleryState
  profile: ProfileState
  onNavigate: (page: string) => void
  /** Jump to a photo's parcel (closes the gallery). */
  onTeleport: (x: number, y: number) => void
  /** Open a captured person's passport. */
  onViewProfile?: (user: ChatUser) => void
}): React.JSX.Element | null {
  const [index, setIndex] = useState<number | null>(null)

  // gallery.list is already sorted newest-first by the session; bucket into month sections.
  const groups = useMemo(() => {
    const out: Group[] = []
    gallery.list.forEach((photo, i) => {
      const key = photo.dateTime ? monthLabel(photoTime(photo.dateTime)) : 'Undated'
      let g = out[out.length - 1]
      if (g == null || g.key !== key) {
        g = { key, items: [] }
        out.push(g)
      }
      g.items.push({ photo, index: i })
    })
    return out
  }, [gallery.list])

  if (!gallery.open) return null

  const p = profile.data
  const selected = index != null ? gallery.list[index] : null
  const pct = gallery.max > 0 ? Math.min(100, (gallery.current / gallery.max) * 100) : 0

  return (
    <MainMenuShell
      active="gallery"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={gallery.toggle}
    >
      <div className={styles.head}>
        <h1 className={styles.title}>Gallery</h1>
        {gallery.loaded && gallery.max > 0 && (
          <div className={styles.storage} title={`${gallery.current} of ${gallery.max} photos used`}>
            <div className={styles.storageBar}>
              <div className={styles.storageFill} style={{ width: `${pct}%` }} />
            </div>
            <span className={styles.storageText}>
              {gallery.current} / {gallery.max} photos
            </span>
          </div>
        )}
      </div>

      {!gallery.loaded ? (
        <div className={styles.center}>
          <Spinner />
        </div>
      ) : gallery.list.length === 0 ? (
        <EmptyState
          variant="screen"
          iconWash
          icon={<Icon name="gallery" size={56} />}
          title="No photos yet"
          subtitle="Snap photos in-world with the camera and they'll show up here."
        />
      ) : (
        <div className={styles.scroll}>
          {groups.map((g) => (
            <section key={g.key} className={styles.group}>
              <h2 className={styles.month}>{g.key}</h2>
              <div className={styles.grid}>
                {g.items.map(({ photo, index: i }) => (
                  <button key={photo.id} type="button" aria-label="Open photo" className={styles.thumb} onClick={() => setIndex(i)}>
                    <img className={styles.thumbImg} src={photo.thumbnailUrl ?? photo.url} alt="" loading="lazy" />
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}

      {selected && index != null && (
        <PhotoDetail
          photos={gallery.list}
          index={index}
          meta={gallery.metas[selected.id]}
          isSelf
          onIndex={setIndex}
          onLoadMeta={gallery.loadPhoto}
          onClose={() => setIndex(null)}
          onTeleport={(x, y) => {
            setIndex(null)
            gallery.toggle() // close the gallery so the teleport lands in-world
            onTeleport(x, y)
          }}
          onDelete={(id) => {
            gallery.remove(id)
            setIndex(null)
          }}
          onViewPerson={onViewProfile}
        />
      )}
    </MainMenuShell>
  )
}

// The Create page: a menu page whose body belongs to the scene editor, which renders its home
// (the creator's scenes) into it. Opening a scene closes the page and takes the screen.

import { useContext, useEffect, useRef } from 'react'
import { showToast, Spinner } from '../../design'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { EngineSession, ProfileState } from '../session/useEngineSession'
import { EditorEntryContext, type EditorEntry } from './entry'
import styles from './CreatePage.module.css'

function Home({ entry, close }: { entry: EditorEntry; close: () => void }): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  const closeRef = useRef(close)
  closeRef.current = close
  const { load } = entry

  useEffect(() => {
    const el = ref.current!
    let gone = false
    let unmount: (() => void) | null = null
    load()
      .then((editor) => {
        if (!gone) unmount = editor.mountHome(el, { close: () => closeRef.current() })
      })
      .catch((e: unknown) => {
        if (gone) return
        console.error('[create] the editor home failed', e)
        showToast('Create could not be loaded. Try again in a moment.', { tone: 'error' })
        closeRef.current()
      })
    return () => {
      gone = true
      try {
        unmount?.()
      } catch (e) {
        console.error('[create] unmounting the editor home failed', e)
      }
    }
  }, [load])

  return (
    <div className={styles.body}>
      <div ref={ref} id="dcl-editor-home" className={styles.home} />
      {entry.loading && (
        <div className={styles.opening} role="status">
          <Spinner size={20} />
          Opening Create…
        </div>
      )}
    </div>
  )
}

export function CreatePage({
  create,
  profile,
  onNavigate
}: {
  create: EngineSession['create']
  profile: ProfileState
  onNavigate: (page: string) => void
}): React.JSX.Element | null {
  const entry = useContext(EditorEntryContext)
  if (!create.open || entry == null) return null
  const p = profile.data
  const close = (): void => create.show(false)
  return (
    <MainMenuShell
      active="create"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={close}
    >
      <Home entry={entry} close={close} />
    </MainMenuShell>
  )
}

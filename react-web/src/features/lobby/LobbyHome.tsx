// The lobby: shown after sign-in, before entering the world. The engine is up and signed in but
// holds the world back, so the avatar preview (drawn by the engine behind this transparent page)
// and the bridge work; picking a destination releases it.

import { Button, DclLogo } from '../../design'
import { EngineViewport } from '../engine/EngineViewport'
import type { Destination } from '../session/useEngineSession'
import styles from './LobbyHome.module.css'

type Rect = { x: number; y: number; width: number; height: number }

export function LobbyHome({
  name,
  onPick,
  setEngineViewport
}: {
  name: string | null
  onPick: (dest: Destination) => void
  setEngineViewport: (region: 'map' | 'avatarPreview', rect: Rect | null, dpr?: number) => void
}): React.JSX.Element {
  return (
    <div className={styles.root}>
      <header className={styles.header}>
        <DclLogo />
      </header>
      <div className={styles.avatar}>
        <EngineViewport region="avatarPreview" report={setEngineViewport} />
      </div>
      <section className={styles.quickJump}>
        <h1 className={styles.welcome}>{name ? `Welcome ${name}!` : 'Welcome!'}</h1>
        <Button variant="primary" size="lg" onClick={() => onPick(null)}>
          Jump in
        </Button>
      </section>
    </div>
  )
}

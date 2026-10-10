// The welcome page: after sign-in, before the lobby (or a realm/position link), while the terms
// aren't accepted on this install or the account has no profile yet. It is where the terms are
// accepted, and where a new account names itself, picks a look, and deploys its profile.
// The stage and the avatar are the lobby's, drawn by the engine behind this transparent page.

import { useEffect, useRef, useState } from 'react'
import logo from '../../assets/lobby/logo.png'
import { Button, TextInput } from '../../design'
import { splitName } from '../../lib/identity'
import { EngineViewport } from '../engine/EngineViewport'
import { StandInStage } from '../lobby/LobbyHome'
import lobbyStyles from '../lobby/LobbyHome.module.css'
import { useStoredProfile } from '../login/useStoredProfile'
import { NAME_MAX, isValidName } from '../profile/profileFields'
import { useSession } from '../session/SessionContext'
import styles from './WelcomePage.module.css'

type Rect = { x: number; y: number; width: number; height: number }

export function WelcomePage({
  setEngineViewport
}: {
  setEngineViewport: (region: 'map' | 'avatarPreview' | 'lobby', rect: Rect | null, dpr?: number) => void
}): React.JSX.Element {
  const session = useSession()
  const { terms, newProfile, pending, saving, error, reroll, accept, openLegal } = session.welcome
  const profile = session.profile.data
  const stored = useStoredProfile(session.login.account ?? undefined)
  // Only the part before the '#': the suffix is the protocol's, not something you type.
  const current = splitName(profile?.name ?? '').base

  // The field starts from the profile once it arrives; after that it is the user's.
  const [typed, setTyped] = useState('')
  const edited = useRef(false)
  useEffect(() => {
    if (!edited.current) setTyped(current)
  }, [current])

  const name = typed.trim()
  // an existing profile only accepts the terms here
  const canAccept = !saving && (!newProfile || (profile != null && isValidName(name)))

  // Until the engine says whether the page is needed, only the background: the avatar and the
  // chrome come with the page or the lobby, whichever it is.
  if (pending) {
    return (
      <div className={lobbyStyles.root}>
        <StandInStage hidden={false} />
      </div>
    )
  }

  return (
    <div className={lobbyStyles.root}>
      <div className={lobbyStyles.stage}>
        <EngineViewport region="lobby" report={setEngineViewport} />
      </div>
      <StandInStage hidden={session.lobbyStageReady} body={stored.body} />

      <header className={lobbyStyles.header}>
        <img className={lobbyStyles.logo} src={logo} alt="Decentraland" />
      </header>

      <section className={styles.card} aria-label="Welcome">
        <h1 className={styles.title}>Welcome to Decentraland</h1>

        {newProfile && (
          <>
            <div className={styles.field}>
              <label className={styles.label} htmlFor="welcome-name">
                Name
              </label>
              <TextInput
                id="welcome-name"
                value={typed}
                maxLength={NAME_MAX}
                invalid={name !== '' && !isValidName(name)}
                disabled={saving}
                onChange={(v) => {
                  edited.current = true
                  setTyped(v)
                }}
              />
              <div className={styles.hint}>Letters and numbers only, up to {NAME_MAX} characters.</div>
            </div>

            <Button variant="secondary" onClick={reroll} disabled={saving}>
              RANDOMIZE
            </Button>
            {terms && <div className={styles.hint}>You can freely customize your appearance after accepting the terms.</div>}
          </>
        )}

        {terms && (
          <p className={styles.terms}>
            By continuing you agree to the{' '}
            <button type="button" className={styles.link} onClick={() => openLegal('terms')}>
              Terms of Use
            </button>{' '}
            and the{' '}
            <button type="button" className={styles.link} onClick={() => openLegal('privacy')}>
              Privacy Policy
            </button>
            .
          </p>
        )}

        {error != null && <div className={styles.error}>{error}</div>}

        <Button
          variant="primary"
          size="lg"
          disabled={!canAccept}
          onClick={() => accept(newProfile ? name : undefined)}
        >
          {terms ? 'ACCEPT AND PLAY' : 'PLAY'}
        </Button>
      </section>
    </div>
  )
}

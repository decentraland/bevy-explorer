// What a visitor with no session sees first: pick a name, shown on the curated default look the
// guest will wear, and play as a guest, which creates the guest account only on ACCEPT AND PLAY; or
// sign in or sign up on the auth site. The welcome page that follows a guest sign-in is answered
// with this pick (useEngineSession exploreAsGuest).

import { useEffect, useState } from 'react'
import logo from '../../assets/lobby/logo.png'
import { Button, TextInput } from '../../design'
import { randomDefaultLook, randomGuestName } from '../../lib/guestName'
import { StandInStage } from '../lobby/LobbyHome'
import { prefetchPlaces } from '../places/placesApi'
import lobbyStyles from '../lobby/LobbyHome.module.css'
import { NAME_MAX, isValidName } from '../profile/profileFields'
import type { LoginFlow } from '../session/useEngineSession'
import welcomeStyles from '../welcome/WelcomePage.module.css'
import { pendingLabel } from './LoadingAndLogin'
import { useStoredProfile } from './useStoredProfile'
import styles from './GuestStartPage.module.css'

const LEGAL = { terms: 'https://decentraland.org/terms/', privacy: 'https://decentraland.org/privacy/' }

export function GuestStartPage({ flow }: { flow: LoginFlow }): React.JSX.Element {
  const [name, setName] = useState(() => randomGuestName())
  // the look the new guest deploys with, shown on the stage meanwhile
  const [look, setLook] = useState(() => randomDefaultLook())
  const { body } = useStoredProfile(`default${look}`)
  // the lobby's places, warmed while the visitor picks a name
  useEffect(() => {
    prefetchPlaces()
  }, [])
  const trimmed = name.trim()
  const enginePending = !flow.engineReady

  return (
    <div className={lobbyStyles.root}>
      <StandInStage hidden={false} body={body} />

      <header className={lobbyStyles.header}>
        <img className={lobbyStyles.logo} src={logo} alt="Decentraland" />
      </header>

      <section className={welcomeStyles.card} aria-label="Pick a name">
        <h1 className={welcomeStyles.title}>Pick a Name</h1>

        <TextInput
          aria-label="Name"
          value={name}
          maxLength={NAME_MAX}
          invalid={trimmed !== '' && !isValidName(trimmed)}
          disabled={flow.busy}
          onChange={setName}
          action={{
            icon: 'dice',
            label: 'Random name and look',
            onClick: () => {
              setName(randomGuestName(trimmed))
              setLook(randomDefaultLook(look))
            }
          }}
        />

        <p className={welcomeStyles.terms}>
          By continuing you agree to the{' '}
          <a className={welcomeStyles.link} href={LEGAL.terms} target="_blank" rel="noopener noreferrer">
            Terms of Use
          </a>{' '}
          and the{' '}
          <a className={welcomeStyles.link} href={LEGAL.privacy} target="_blank" rel="noopener noreferrer">
            Privacy Policy
          </a>
          .
        </p>

        {flow.error != null && <div className={welcomeStyles.error}>{flow.error}</div>}

        <Button
          variant="primary"
          size="lg"
          disabled={flow.busy || enginePending || !isValidName(trimmed)}
          onClick={() => flow.exploreAsGuest({ name: trimmed, look })}
        >
          {enginePending ? pendingLabel(flow) : 'ACCEPT AND PLAY'}
        </Button>

        <div className={styles.or}>or</div>

        <Button variant="secondary" size="lg" disabled={flow.busy} onClick={flow.startWithAccount}>
          CONTINUE WITH AN ACCOUNT
        </Button>
      </section>
    </div>
  )
}

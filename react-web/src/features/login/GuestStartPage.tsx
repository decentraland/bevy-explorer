// What a visitor with no session sees first: pick a name, shown on the curated default look the
// guest will wear, and play as a guest, which creates the guest account only on ACCEPT AND PLAY; or
// sign in or sign up on the auth site. The welcome page that follows a guest sign-in is answered
// with this pick (useEngineSession exploreAsGuest).
//
// The look shows as its snapshot until the engine, launched as soon as it is ready, has the lobby
// stage up with the live avatar wearing it (useEngineSession, the scene's avatarPreview). The look
// the dice would show next is fetched ahead, so the stage can load it out of sight.

import { useEffect, useState } from 'react'
import logo from '../../assets/lobby/logo.png'
import { Button, ControlButton, Icon, TextInput } from '../../design'
import { randomDefaultLook, randomGuestName } from '../../lib/guestName'
import { EngineViewport } from '../engine/EngineViewport'
import { StandInStage } from '../lobby/LobbyHome'
import { prefetchPlaces } from '../places/placesApi'
import lobbyStyles from '../lobby/LobbyHome.module.css'
import { NAME_MAX, isValidName } from '../profile/profileFields'
import { useSession } from '../session/SessionContext'
import type { LoginFlow } from '../session/useEngineSession'
import welcomeStyles from '../welcome/WelcomePage.module.css'
import { pendingLabel } from './LoadingAndLogin'
import { useStoredProfile } from './useStoredProfile'
import styles from './GuestStartPage.module.css'

const LEGAL = { terms: 'https://decentraland.org/terms/', privacy: 'https://decentraland.org/privacy/' }
// the two dice, big: the page is all about them
const DICE_ICON = 54

export function GuestStartPage({ flow }: { flow: LoginFlow }): React.JSX.Element {
  const session = useSession()
  const [name, setName] = useState(() => randomGuestName())
  // the look the new guest deploys with, shown on the stage meanwhile, and the dice's next one
  const [{ look, next }, setLooks] = useState(() => {
    const look = randomDefaultLook(session.welcome.lastLook ?? undefined)
    return { look, next: randomDefaultLook(look) }
  })
  const { body, look: avatar } = useStoredProfile(`default${look}`)
  const { look: nextAvatar } = useStoredProfile(`default${next}`)
  // the stage's avatar wears the pick once fetched (queued for the scene until it is up), and
  // loads the next one out of sight
  const { showLook, preloadLook, expectLook } = session.welcome
  useEffect(() => {
    if (avatar != null) showLook(look, avatar)
  }, [look, avatar, showLook])
  useEffect(() => {
    if (nextAvatar != null) preloadLook(next, nextAvatar)
  }, [next, nextAvatar, preloadLook])
  // the lobby's places, warmed while the visitor picks a name
  useEffect(() => {
    prefetchPlaces()
  }, [])
  const trimmed = name.trim()
  // the click is taken: the engine's boot progress shows on the button until it runs
  const queued = flow.acceptQueued
  const locked = flow.busy || queued
  // the look's dice waits for the look it asked for to land on the live stage (the snapshot is
  // instant); the name's dice never waits
  const rerolling = session.welcome.lookPending && session.lobbyStageReady

  return (
    <div className={lobbyStyles.root}>
      <div className={lobbyStyles.stage}>
        <EngineViewport region="lobby" report={session.setEngineViewport} />
      </div>
      <StandInStage hidden={session.lobbyStageReady} body={body} />
      <ControlButton
        variant="solid"
        shape="circle"
        size="lg"
        className={styles.lookDice}
        aria-label="Random look"
        title="Random look"
        disabled={locked || rerolling}
        onClick={() => {
          expectLook()
          setLooks(({ next }) => ({ look: next, next: randomDefaultLook(next) }))
        }}
      >
        <Icon name="dice" size={DICE_ICON} />
      </ControlButton>

      <header className={lobbyStyles.header}>
        <img className={lobbyStyles.logo} src={logo} alt="Decentraland" />
      </header>

      <section className={welcomeStyles.card} aria-label="Pick a name">
        <h1 className={welcomeStyles.title}>Pick a Name</h1>

        <TextInput
          aria-label="Name"
          size="lg"
          value={name}
          maxLength={NAME_MAX}
          invalid={trimmed !== '' && !isValidName(trimmed)}
          disabled={locked}
          onChange={setName}
          action={{
            icon: 'dice',
            label: 'Random name',
            size: DICE_ICON,
            onClick: () => setName(randomGuestName(trimmed))
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
          disabled={locked || !isValidName(trimmed)}
          onClick={() => flow.exploreAsGuest({ name: trimmed, look })}
        >
          {queued && !flow.engineReady ? pendingLabel(flow) : 'ACCEPT AND PLAY'}
        </Button>

        <div className={styles.or}>or</div>

        <Button variant="secondary" size="lg" disabled={locked} onClick={flow.startWithAccount}>
          CONTINUE WITH AN ACCOUNT
        </Button>
      </section>
    </div>
  )
}

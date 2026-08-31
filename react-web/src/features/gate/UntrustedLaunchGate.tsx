// Interstitial for a link that carries an infrastructure-pointing parameter: its own super-user
// scene (`?systemScene=` — see lib/systemScene.ts for why that is worth stopping on) and/or its
// own backend deployment (`?baseDomain=` — see lib/baseDomain.ts).
//
// Not dismissible: ModalShell is scrimless (its host owns the overlay, Escape and focus — see
// design/Modal.tsx), so this gate draws its own inert full-screen layer and `closeButton={false}`
// drops the X — no dismiss affordance exists. It renders INSTEAD of the app (App.tsx returns it
// before EngineHost mounts, so no engine boots and no scene loads while it is up), so there is
// nothing behind it to dismiss to.
//
// Continuing is behind ADVANCED because the safe choice has to be the easy one — the user arrives
// here having already clicked something the attacker gave them.

import { useState } from 'react'
import { Button, DclLogo, ModalShell } from '../../design'
import styles from './UntrustedLaunchGate.module.css'

// A tab the user opened themselves can't be closed by script, so send them somewhere safe instead.
function exitApplication(): void {
  window.close()
  window.location.replace('https://decentraland.org')
}

const TITLE = 'This Launch Link Is Not Trusted'

export function UntrustedLaunchGate({
  systemScene,
  baseDomain,
  onProceed
}: {
  systemScene?: string
  baseDomain?: string
  onProceed: () => void
}): React.JSX.Element {
  const [advanced, setAdvanced] = useState(false)

  return (
    <div className={styles.layer}>
      <ModalShell
        role="alertdialog"
        ariaLabel={TITLE}
        width={560}
        closeButton={false}
        header={
          <div className={styles.head}>
            <DclLogo size={72} />
            <h2 className={styles.title}>{TITLE}</h2>
          </div>
        }
        actionsDirection="column"
        actions={
          <>
            {/* autoFocus lands focus on the safe action rather than whatever the browser picks. */}
            <Button autoFocus variant="primary" className={styles.exit} onClick={exitApplication}>
              Exit Application
            </Button>
            {advanced ? (
              <div className={styles.advanced}>
                <p className={styles.advancedNote}>
                  Continuing hands this address full control of your session for as long as this tab is
                  open. Only do this if you recognise it as your own.
                </p>
                <Button variant="ghost" className={styles.proceed} onClick={onProceed}>
                  Continue anyway
                </Button>
              </div>
            ) : (
              <Button variant="secondary" onClick={() => setAdvanced(true)}>
                Advanced
              </Button>
            )}
          </>
        }
      >
        <p className={styles.lead}>Someone may be trying to change how your Explorer behaves.</p>
        <p className={styles.lead}>
          This link carries {systemScene != null && baseDomain != null ? 'parameters' : 'a parameter'} the
          Explorer does not accept from links:
        </p>

        <dl className={styles.params}>
          {systemScene != null && (
            <>
              <dt>
                systemScene = <span className={styles.paramValue}>{systemScene}</span>
              </dt>
              <dd className={styles.paramDesc}>
                Replaces the Explorer&apos;s interface with a scene loaded from this address. It can move
                your avatar, change your profile, and answer permission prompts on your behalf.
              </dd>
            </>
          )}
          {baseDomain != null && (
            <>
              <dt>
                baseDomain = <span className={styles.paramValue}>{baseDomain}</span>
              </dt>
              <dd className={styles.paramDesc}>
                Points every backend service — sign-in, content, comms — at servers under this domain.
                Whoever runs them would see your session and control what you play.
              </dd>
            </>
          )}
        </dl>

        <p className={styles.lead}>Unless you built this link yourself, the safe choice is to exit.</p>
      </ModalShell>
    </div>
  )
}

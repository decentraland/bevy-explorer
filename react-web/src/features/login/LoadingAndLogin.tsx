// React port of scene/src/ui-classes/loading-and-login/LoadingAndLogin.tsx.
// Sign-in is now same-domain SSO: "Start with account" / "Use different account" bounce to
// the auth site; a stored identity shows the "welcome back" screen and "Jump in" hands the
// identity straight to the engine. No verification-code / secure-step screen anymore.

import { useEffect, useState } from 'react'
import { Button } from '../../design'
import { prefetchPlaces } from '../places/placesApi'
import type { LoginFlow, LoginStatus } from '../session/useEngineSession'
import styles from './LoadingAndLogin.module.css'

// Engine boot steps surfaced from the engine loader (deploy/web/ui.js), shown in the footer bar.
const STEP_LABEL: Record<string, string> = {
  download: 'Downloading engine',
  compile: 'Compiling',
  init: 'Initializing',
  workers: 'Starting workers',
  gpu: 'Preparing graphics'
}

// Footer progress bar driven by the engine's REAL weighted boot progress (the bundle's own bar is
// hidden via hideLoader=1). Visible only while the engine is still booting; the user can pick an
// account meanwhile, and "Jump in" / "Explore as guest" un-gate when it reaches ready.
function BootProgress({ flow }: { flow: LoginFlow }): React.JSX.Element | null {
  if (flow.engineReady) return null
  const label = (flow.loadStep != null ? STEP_LABEL[flow.loadStep] : undefined) ?? 'Starting…'
  const pct = Math.round(flow.loadProgress)
  return (
    <div className={styles.bootBar} role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label={label}>
      <div className={styles.bootLabel}>
        {label} · {pct}%
      </div>
      <div className={styles.bootTrack}>
        <div className={styles.bootFill} style={{ width: `${pct}%` }} />
      </div>
    </div>
  )
}

// Pending-CTA label: surface the real boot progress while the WASM downloads (the long phase),
// e.g. "DOWNLOADING… 47%", then a generic "STARTING…" for the quick compile/GPU tail. The percent
// is the overall weighted progress (download is weighted ~80%), matching the footer bar.
function pendingLabel(flow: LoginFlow): string {
  if (flow.loadStep === 'download') return `DOWNLOADING… ${Math.round(flow.loadProgress)}%`
  return 'STARTING…'
}

// Circled arrow on the primary CTA (Figma "JUMP IN" button): white disc + brand arrow.
function ArrowIcon(): React.JSX.Element {
  return (
    <span className={styles.arrow} aria-hidden="true">
      <svg viewBox="0 0 28 28" width="22" height="22">
        <circle cx="14" cy="14" r="14" fill="#fff" />
        <path d="M12.5 9l5 5-5 5M9 14h8" stroke="var(--brand)" strokeWidth="2.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </span>
  )
}

// The stored account's name + full-body snapshot. The by-address image redirects to a broken
// S3 path, so we resolve the real (hash-based) body URL from the profile (lambdas send CORS →
// fetch works under credentialless).
interface ProfileResponse {
  avatars?: Array<{ name?: string; avatar?: { snapshots?: { body?: string } } }>
}
function useProfile(address?: string): { name?: string; body?: string } {
  const [data, setData] = useState<{ name?: string; body?: string }>({})
  useEffect(() => {
    if (address == null || address === '') {
      setData({})
      return
    }
    let cancelled = false
    fetch(`https://peer.decentraland.org/lambdas/profiles/${address}`)
      .then((r) => r.json() as Promise<ProfileResponse>)
      .then((j) => {
        const a = j.avatars?.[0]
        if (!cancelled) setData({ name: a?.name, body: a?.avatar?.snapshots?.body })
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [address])
  return data
}

// The previous account's avatar: a static full-body snapshot on a CSS gold podium. (The live 3D
// avatar can't render pre-login — the engine has no spawned avatar yet — so we use the snapshot.)
function LoginAvatar({ body }: { body: string }): React.JSX.Element | null {
  const [failed, setFailed] = useState(false)
  if (failed) return null
  return (
    <div className={styles.avatarWrap}>
      <div className={styles.disc} />
      <div className={styles.glow} />
      <img className={styles.avatar} src={body} alt="" draggable={false} onError={() => setFailed(true)} />
    </div>
  )
}

const COPY: Record<Exclude<LoginStatus, 'loading'>, { title: string; subtitle?: string }> = {
  'sign-in-or-guest': {
    title: 'Discover a virtual social world',
    subtitle: 'shaped by its community of\ncreators & explorers.'
  },
  'reuse-login-or-new': {
    title: 'Welcome back!',
    subtitle: 'Ready to explore?'
  }
}

export function LoadingAndLogin({ flow }: { flow: LoginFlow }): React.JSX.Element {
  // Warm the default places list now, while the user is on the login screen, so the post-jump-in
  // picker renders with data instead of a spinner.
  useEffect(() => {
    prefetchPlaces()
  }, [])
  const reuse = flow.status === 'reuse-login-or-new'
  const profile = useProfile(reuse && flow.account != null ? flow.account : undefined)

  return (
    <div className={styles.root}>
      {flow.status === 'loading' ? (
        <div className={styles.loading}>
          <div className={styles.spinnerLogo} />
        </div>
      ) : (
        <>
          <div className={styles.watermark} />
          <Panel flow={flow} name={profile.name} />
          {reuse && profile.body != null && <LoginAvatar body={profile.body} />}
          <BootProgress flow={flow} />
        </>
      )}
    </div>
  )
}

function Panel({ flow, name }: { flow: LoginFlow; name?: string }): React.JSX.Element {
  const status = flow.status as Exclude<LoginStatus, 'loading'>
  const copy = COPY[status]
  const title = status === 'reuse-login-or-new' && name != null && name !== '' ? `Welcome back ${name}` : copy.title
  // The engine boots in the background while this screen is up. Keep the engine-driven CTAs disabled
  // (showing a starting state) until it can take the login command, so a click never lands in a
  // silent wait. The auth-redirect buttons don't touch the engine, so they stay enabled.
  const enginePending = !flow.engineReady

  return (
    <div className={styles.panel}>
      <div className={styles.logo} />
      <h1 className={styles.title}>{title}</h1>
      {copy.subtitle && <p className={styles.subtitle}>{copy.subtitle}</p>}
      {flow.error && <p className={styles.error}>{flow.error}</p>}

      <div className={styles.buttons}>
        {flow.authPending && (
          // Native fresh sign-in (engine remote-wallet flow): the engine opened the auth site
          // in the user's external browser; show the verification code to match there.
          <>
            <p className={styles.authHint}>
              {flow.authCode != null
                ? 'Verify this code matches the one in the browser window we just opened:'
                : 'Opening the sign-in page in your browser…'}
            </p>
            {flow.authCode != null && <div className={styles.authCode}>{flow.authCode}</div>}
            <Button variant="secondary" size="lg" className={styles.ctaSecondary} onClick={flow.cancelLogin}>
              <span className={styles.label}>CANCEL</span>
            </Button>
          </>
        )}
        {status === 'sign-in-or-guest' && !flow.authPending && (
          <>
            <Button variant="primary" size="lg" className={styles.cta} onClick={flow.startWithAccount} disabled={flow.busy}>
              <span className={styles.label}>START WITH ACCOUNT</span>
              <ArrowIcon />
            </Button>
            <Button variant="secondary" size="lg" className={styles.ctaSecondary} onClick={flow.exploreAsGuest} disabled={flow.busy || enginePending}>
              <span className={styles.label}>{enginePending ? pendingLabel(flow) : 'EXPLORE AS GUEST'}</span>
            </Button>
          </>
        )}

        {status === 'reuse-login-or-new' && (
          <>
            <Button variant="primary" size="lg" className={styles.cta} onClick={flow.jumpIn} disabled={flow.busy || enginePending}>
              <span className={styles.label}>{enginePending ? pendingLabel(flow) : flow.profileFetchFailed ? 'TRY AGAIN' : 'JUMP INTO DECENTRALAND'}</span>
              {!enginePending && <ArrowIcon />}
            </Button>
            <Button variant="secondary" size="lg" className={styles.ctaSecondary} onClick={flow.useDifferentAccount} disabled={flow.busy}>
              <span className={styles.label}>USE A DIFFERENT ACCOUNT</span>
            </Button>
            {flow.profileFetchFailed && (
              <div className={styles.resetBox}>
                <p className={styles.resetText}>
                  If the problem persists you can continue with a fresh default profile.
                  This permanently replaces your account&apos;s current avatar and profile.
                </p>
                <Button
                  variant="secondary"
                  size="lg"
                  className={`${styles.ctaSecondary} ${styles.ctaDanger}`}
                  onClick={flow.resetProfileAndJumpIn}
                  disabled={flow.busy || enginePending}
                >
                  <span className={styles.label}>RESET PROFILE & JUMP IN</span>
                </Button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  )
}

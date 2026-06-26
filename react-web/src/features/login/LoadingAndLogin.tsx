// React port of scene/src/ui-classes/loading-and-login/LoadingAndLogin.tsx.
// Sign-in is now same-domain SSO: "Start with account" / "Use different account" bounce to
// the auth site; a stored identity shows the "welcome back" screen and "Jump in" hands the
// identity straight to the engine. No verification-code / secure-step screen anymore.

import { useEffect, useState } from 'react'
import { Button } from '../../design'
import type { LoginFlow, LoginStatus } from '../session/useEngineSession'
import styles from './LoadingAndLogin.module.css'

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
        </>
      )}
    </div>
  )
}

function Panel({ flow, name }: { flow: LoginFlow; name?: string }): React.JSX.Element {
  const status = flow.status as Exclude<LoginStatus, 'loading'>
  const copy = COPY[status]
  const title = status === 'reuse-login-or-new' && name != null && name !== '' ? `Welcome back ${name}` : copy.title

  return (
    <div className={styles.panel}>
      <div className={styles.logo} />
      <h1 className={styles.title}>{title}</h1>
      {copy.subtitle && <p className={styles.subtitle}>{copy.subtitle}</p>}
      {flow.error && <p className={styles.error}>{flow.error}</p>}

      <div className={styles.buttons}>
        {status === 'sign-in-or-guest' && (
          <>
            <Button variant="primary" size="lg" className={styles.cta} onClick={flow.startWithAccount} disabled={flow.busy}>
              START WITH ACCOUNT
              <ArrowIcon />
            </Button>
            <Button variant="secondary" size="lg" className={styles.ctaSecondary} onClick={flow.exploreAsGuest} disabled={flow.busy}>
              EXPLORE AS GUEST
            </Button>
          </>
        )}

        {status === 'reuse-login-or-new' && (
          <>
            <Button variant="primary" size="lg" className={styles.cta} onClick={flow.jumpIn} disabled={flow.busy}>
              JUMP INTO DECENTRALAND
              <ArrowIcon />
            </Button>
            <Button variant="secondary" size="lg" className={styles.ctaSecondary} onClick={flow.useDifferentAccount} disabled={flow.busy}>
              USE A DIFFERENT ACCOUNT
            </Button>
          </>
        )}
      </div>
    </div>
  )
}

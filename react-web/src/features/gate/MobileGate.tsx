// Gate — shown instead of the HUD when the engine can't run: on a mobile browser (→ native apps) or
// a non-Chromium desktop browser (→ "use Chrome"). Copy, store links, and the Chrome bypass mirror
// the engine bundle's own gate (deploy/web/index.html) so the two stay consistent.
//
// KEEP IN SYNC with deploy/web/index.html: the store/Chrome URLs and the Apple/Google/Chrome SVGs are
// duplicated there (the engine loader gates before react-web mounts, so it can't import from here).

import { mobilePlatform } from '../../lib/isMobile'
import { publicUrl } from '../../lib/publicUrl'
import { Button } from '../../design'
import styles from './MobileGate.module.css'

const APP_STORE_URL = 'https://testflight.apple.com/join/KF4r3jlU'
const PLAY_STORE_URL = 'https://play.google.com/store/apps/details?id=org.decentraland.godotexplorer'
const CHROME_URL = 'https://www.google.com/chrome/'

function AppleIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z" />
    </svg>
  )
}

function GooglePlayIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M3,20.5V3.5C3,2.91 3.34,2.39 3.84,2.15L13.69,12L3.84,21.85C3.34,21.6 3,21.09 3,20.5M16.81,15.12L6.05,21.34L14.54,12.85L16.81,15.12M20.16,10.81C20.5,11.08 20.75,11.5 20.75,12C20.75,12.5 20.53,12.9 20.18,13.18L17.89,14.5L15.39,12L17.89,9.5L20.16,10.81M6.05,2.66L16.81,8.88L14.54,11.15L6.05,2.66Z" />
    </svg>
  )
}

function ChromeIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 48 48" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="cg-a" x1="3.2173" y1="15" x2="44.7812" y2="15" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#d93025" />
          <stop offset="1" stopColor="#ea4335" />
        </linearGradient>
        <linearGradient id="cg-b" x1="20.7219" y1="47.6791" x2="41.5039" y2="11.6837" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#fcc934" />
          <stop offset="1" stopColor="#fbbc04" />
        </linearGradient>
        <linearGradient id="cg-c" x1="26.5981" y1="46.5015" x2="5.8161" y2="10.506" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#1e8e3e" />
          <stop offset="1" stopColor="#34a853" />
        </linearGradient>
      </defs>
      <circle cx="24" cy="23.9947" r="12" fill="#fff" />
      <path d="M24,12H44.7812a23.9939,23.9939,0,0,0-41.5639.0029L13.6079,30l.0093-.0024A11.9852,11.9852,0,0,1,24,12Z" fill="url(#cg-a)" />
      <circle cx="24" cy="24" r="9.5" fill="#1a73e8" />
      <path d="M34.3913,30.0029,24.0007,48A23.994,23.994,0,0,0,44.78,12.0031H23.9989l-.0025.0093A11.985,11.985,0,0,1,34.3913,30.0029Z" fill="url(#cg-b)" />
      <path d="M13.6086,30.0031,3.218,12.006A23.994,23.994,0,0,0,24.0025,48L34.3931,30.0029l-.0067-.0068a11.9852,11.9852,0,0,1-20.7778.007Z" fill="url(#cg-c)" />
    </svg>
  )
}

// "try anyway" — mirror the engine gate's escape hatch: set the shared bypass cookie + reload.
function tryAnyway(): void {
  document.cookie = 'bypass_browser_check=1;path=/;max-age=2592000'
  location.reload()
}

export function MobileGate({ reason = 'mobile' }: { reason?: 'mobile' | 'browser' }): React.JSX.Element {
  if (reason === 'browser') {
    return (
      <div className={styles.root}>
        <div className={styles.card}>
          <img className={styles.logo} src={publicUrl('assets/logo.png')} alt="" draggable={false} />
          <h1 className={styles.title}>Browser Not Supported</h1>
          <p className={styles.subtitle}>
            Decentraland Web requires <strong>Google Chrome</strong> on desktop to run.
          </p>
          <div className={styles.buttons}>
            <Button variant="secondary" href={CHROME_URL} target="_blank" rel="noopener" className={styles.store}>
              <ChromeIcon />
              <span>Download Chrome</span>
            </Button>
          </div>
          {/* Bespoke text-link, not a Button: no link/text variant exists yet (Button is primary/
              secondary/ghost pills) — tracked in docs/design-system-backlog.md (Button extend). */}
          <button type="button" className={styles.tryAnyway} onClick={tryAnyway}>
            try anyway…
          </button>
        </div>
      </div>
    )
  }

  const platform = mobilePlatform()
  const showApple = platform === 'ios' || platform === 'other'
  const showGoogle = platform === 'android' || platform === 'other'
  return (
    <div className={styles.root}>
      <div className={styles.card}>
        <img className={styles.logo} src={publicUrl('assets/logo.png')} alt="" draggable={false} />
        <h1 className={styles.title}>Decentraland</h1>
        <p className={styles.subtitle}>
          This experience isn’t available on mobile browsers. Download the app to explore:
        </p>
        <div className={styles.buttons}>
          {showApple && (
            <Button variant="secondary" href={APP_STORE_URL} target="_blank" rel="noopener" className={styles.store}>
              <AppleIcon />
              <span>Install from App Store</span>
            </Button>
          )}
          {showGoogle && (
            <Button variant="secondary" href={PLAY_STORE_URL} target="_blank" rel="noopener" className={styles.store}>
              <GooglePlayIcon />
              <span>Install from Google Play</span>
            </Button>
          )}
        </div>
      </div>
    </div>
  )
}

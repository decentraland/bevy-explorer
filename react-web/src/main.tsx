import './lib/noLocalStorage' // first: before anything can read localStorage
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/inter/index.css' // self-hosted Inter (matches the Figma type)
import { App } from './App'
import { registerCoiServiceWorker } from './lib/coiServiceWorker'
import { installCefNativeBridge } from './lib/cefNativeBridge'
import { installLocalNetworkFetch } from './lib/localNetworkFetch'
import { installHudScale } from './lib/hudScale'
import { countLaunch } from './lib/launchCount'
import { isNativeHud } from './lib/bootMode'
import { inShell } from './lib/shell'
import { loadPrefs } from './lib/prefs'
import './styles/global.css'

// Before anything fetches: annotate loopback/local-network requests so Chrome's Local Network
// Access permission (142+) prompts instead of silently blocking — preview realms live on localhost.
installLocalNetworkFetch()

// Keep --ui-scale in sync with the viewport (DPI-correct, like Unity's CanvasScaler).
installHudScale()

let redirecting = false
// NATIVE (?native=1): bevy renders the 3D world *behind* this transparent webview, so the page must
// be transparent (in web mode the engine canvas lives in this document at z-0, so the body
// background is fine).
if (isNativeHud()) {
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
  // No webview context menu (Back/Reload) on right-click — that gesture is the engine's camera.
  window.addEventListener('contextmenu', (e) => e.preventDefault())
  // CEF host (react-hud-cef): bridge Envelopes over window.cef (no-op when it's absent).
  installCefNativeBridge()
  // Some webview backends treat a GPU-overlay webview as a background page and throttle rendering;
  // report the page as always visible so rAF/paint keep running.
  try {
    Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true })
    Object.defineProperty(document, 'hidden', { get: () => false, configurable: true })
    document.dispatchEvent(new Event('visibilitychange'))
  } catch {
    /* defineProperty can throw if already overridden; non-fatal */
  }
} else if (!inShell && import.meta.env.PROD) {
  // On the web the app belongs inside the shell (index.html), which keeps the sign-in key out of
  // this page's process; opened on its own it would read the key itself.
  redirecting = true
  location.replace(new URL('./', location.href).href + location.search + location.hash)
} else if (!inShell) {
  // Dev (web) only: swap the host's COEP require-corp for credentialless via the shared root SW
  // (catalyst <img> thumbnails send no CORP). Never in the native webview. Inside the shell, the
  // shell registers it.
  registerCoiServiceWorker()
}

// App picks the mode: ?mock=1 → login UI against the fake bridge (no engine);
// default → real engine in a same-document canvas driven over console commands.
// The HUD's stored values are read before it renders (lib/prefs).
if (!redirecting) void loadPrefs().then(() => {
  countLaunch()
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>
  )
})

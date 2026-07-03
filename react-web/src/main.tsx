import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/inter/index.css' // self-hosted Inter (matches the Figma type)
import { App } from './App'
import { registerCoiServiceWorker } from './lib/coiServiceWorker'
import './styles/global.css'

// NATIVE (?native=1): bevy renders the 3D world *behind* this transparent webview, so the page must
// be transparent (in web mode the engine canvas lives in this document at z-0, so the body
// background is fine).
if (new URLSearchParams(location.search).get('native') === '1') {
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
  // No webview context menu (Back/Reload) on right-click — that gesture is the engine's camera.
  window.addEventListener('contextmenu', (e) => e.preventDefault())
  // Some webview backends treat a GPU-overlay webview as a background page and throttle rendering;
  // report the page as always visible so rAF/paint keep running.
  try {
    Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true })
    Object.defineProperty(document, 'hidden', { get: () => false, configurable: true })
    document.dispatchEvent(new Event('visibilitychange'))
  } catch {
    /* defineProperty can throw if already overridden; non-fatal */
  }
} else {
  // Prod-only (web): swap the host's COEP require-corp for credentialless via the shared root SW
  // (catalyst <img> thumbnails send no CORP). No-op in dev; never in the native webview.
  registerCoiServiceWorker()
}

// App picks the mode: ?mock=1 → login UI against the fake bridge (no engine);
// default → real engine in a same-origin iframe driven over console commands.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>
)

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/inter/index.css' // self-hosted Inter (matches the Figma type)
import { App } from './App'
import { PopupHost } from './design'
import { registerCoiServiceWorker } from './lib/coiServiceWorker'
import './styles/global.css'

// Prod-only: swap the host's COEP require-corp for credentialless via the shared root SW
// (catalyst <img> thumbnails send no CORP). No-op in dev.
registerCoiServiceWorker()

// App picks the mode: ?mock=1 → login UI against the fake bridge (no engine);
// default → real engine in a same-origin iframe driven over console commands.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
    <PopupHost />
  </StrictMode>
)

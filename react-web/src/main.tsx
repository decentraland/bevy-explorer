import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/inter/index.css' // self-hosted Inter (matches the Figma type)
import { App } from './App'
import './styles/global.css'

// App picks the mode: ?mock=1 → login UI against the fake bridge (no engine);
// default → real engine in a same-origin iframe driven over console commands.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>
)

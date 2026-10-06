// Inside the web shell, nothing in the app may read localStorage, our code or a library's: the
// first read loads the origin's whole storage area into this process, and it holds the sign-in key
// the shell keeps (src/shell/main.ts). Imported first by main.tsx, so this runs before anything
// else has looked; the HUD's own values live in lib/prefs.
import { inShell } from './shell'

if (inShell) {
  Object.defineProperty(window, 'localStorage', {
    configurable: false,
    get() {
      throw new Error('localStorage is not available to the app: use lib/prefs')
    }
  })
}

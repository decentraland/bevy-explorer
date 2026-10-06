// The web shell (index.html): the top-level page that embeds the app (app.html: HUD + engine) in
// an iframe. app.html is served with Document-Isolation-Policy, which makes it cross-origin
// isolated on its own; this page is deliberately NOT isolated, so the browser keeps the two in
// separate processes. The signed-in identity's key stays here: the app asks for the public
// identity and for signatures, and never reads the origin's localStorage itself.
import { authLoginUrl, clearStoredLogins, findStoredIdentity, getStoredLogin, publicLogin } from '../features/auth/sso'
import { migratePrefs } from '../lib/prefs'
import { personalSign } from './sign'

// The service worker's scope is the package DIRECTORY, but the production entry URL has no
// trailing slash, which puts this page outside it. Canonicalize to the directory form; the
// one-shot reload below then navigates in scope.
if (!location.pathname.endsWith('/')) {
  history.replaceState(history.state, '', location.pathname + '/' + location.search + location.hash)
}

let frame: HTMLIFrameElement | null = null
let mounting = false
function mount(): void {
  if (mounting) return
  mounting = true
  // the app can't read localStorage, so hand it whatever an earlier version kept there first
  void migratePrefs()
    .catch((e: unknown) => console.warn('[shell] could not move stored preferences:', e))
    .then(mountApp)
}

function mountApp(): void {
  const f = document.createElement('iframe')
  f.src = 'app.html' + location.search + location.hash
  f.allow = 'fullscreen; microphone; clipboard-read; clipboard-write; autoplay; gamepad'
  f.addEventListener('load', () => f.focus())
  document.body.appendChild(f)
  frame = f
}

function request(method: unknown, params: unknown): unknown {
  switch (method) {
    case 'identity':
      return publicLogin(getStoredLogin())
    case 'sign': {
      const { signer, message } = (params ?? {}) as { signer?: unknown; message?: unknown }
      if (typeof signer !== 'string' || typeof message !== 'string') throw new Error('bad sign request')
      const key = findStoredIdentity(signer)?.ephemeralIdentity.privateKey
      if (!key) throw new Error(`no stored identity for ${signer}`)
      return personalSign(key, message)
    }
    case 'logout':
      clearStoredLogins()
      return null
    default:
      throw new Error(`unknown request ${String(method)}`)
  }
}

// Messages from the app, for things only this page can do.
addEventListener('message', (e: MessageEvent) => {
  if (!frame || e.source !== frame.contentWindow || e.origin !== location.origin) return
  const m = e.data as { type?: unknown; [key: string]: unknown } | null
  if (!m || typeof m !== 'object') return
  switch (m.type) {
    case 'bevy-shell:request': {
      let reply: { result?: unknown; error?: string }
      try {
        reply = { result: request(m.method, m.params) }
      } catch (err) {
        reply = { error: err instanceof Error ? err.message : String(err) }
      }
      frame.contentWindow?.postMessage({ type: 'bevy-shell:response', id: m.id, ...reply }, location.origin)
      break
    }
    // the engine's URL sync (engine/boot.js set_url_params), mirrored into the address bar
    case 'bevy-shell:url':
      if (typeof m.search === 'string' && (m.search === '' || m.search[0] === '?')) {
        history.replaceState(null, '', location.pathname + m.search)
      }
      break
    // sign-in: the same-origin auth site, returning here
    case 'bevy-shell:auth-login':
      location.replace(authLoginUrl(location.href))
      break
    // the untrusted-link gate's exit (see src/features/gate/UntrustedLaunchGate.tsx)
    case 'bevy-shell:exit':
      window.close()
      location.replace('https://decentraland.org')
      break
  }
})

// The service worker gives app.html its isolation header and strips the host's COOP/COEP from
// this page (deploy/web/service_worker.js), and the engine's IPFS cache depends on it. A first
// visit or a hard reload bypasses it, so reload ONCE when it is active; the flag stops a broken
// worker from reload-looping. Until then the app is not mounted.
const FLAG = 'coi_sw_reloaded'
if (!('serviceWorker' in navigator)) {
  mount()
} else {
  // whether THIS load went through the worker: one that claims the page mid-load (clients.claim
  // on a first visit) didn't rewrite its headers, so it still needs the reload
  const controlled = navigator.serviceWorker.controller != null
  if (controlled) {
    sessionStorage.removeItem(FLAG)
    mount()
  }
  navigator.serviceWorker
    .register('service_worker.js')
    .then((reg) => {
      if (controlled) return
      const reloadOnce = (): void => {
        if (sessionStorage.getItem(FLAG)) {
          sessionStorage.removeItem(FLAG)
          console.error('[shell] service worker failed to take control after reload')
          mount()
        } else {
          sessionStorage.setItem(FLAG, 'true')
          location.reload()
        }
      }
      if (reg.active) return reloadOnce()
      const waiting = reg.installing ?? reg.waiting
      if (!waiting) return mount()
      waiting.addEventListener('statechange', () => {
        if (waiting.state === 'activated') reloadOnce()
      })
    })
    .catch((e: unknown) => {
      console.log('[shell] service worker registration failed:', e)
      mount()
    })
}

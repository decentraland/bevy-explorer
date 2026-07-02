// COOP/COEP escape hatch for production. The hosting site (decentraland.zone/bevy-web) serves
// `COEP: require-corp`, which blocks plain <img> loads from catalysts (they send no CORP header).
// The shared service worker at the PACKAGE ROOT (deploy/web/service_worker.js) rewrites
// same-origin navigations to `COEP: credentialless`, which allows credential-less no-CORP images
// while keeping crossOriginIsolated for the engine. The engine page registers the SAME script
// (engine/main.js), so one registration scopes both.
//
// Dev is a no-op: Vite already serves credentialless headers directly and doesn't host the SW.
import { PAGE_DIR } from './publicUrl'

// One-shot reload guard. Deliberately NOT the engine's `sw_reloaded` — the iframe shares this
// origin's sessionStorage and runs its own copy of the dance (engine/main.js).
const FLAG = 'coi_sw_reloaded'

export function registerCoiServiceWorker(): void {
  if (!import.meta.env.PROD || !('serviceWorker' in navigator)) return

  // The SW scope is the package DIRECTORY (…/bevy-web/), but the production entry URL has no
  // trailing slash (…/bevy-web) — which is OUTSIDE that scope, so the page would never be
  // controlled and would keep the host's require-corp COEP forever. Canonicalize the URL to the
  // directory form (keeps query/hash); the one-shot reload below then navigates in scope.
  if (!location.pathname.endsWith('/')) {
    history.replaceState(history.state, '', `${location.pathname}/${location.search}${location.hash}`)
  }

  navigator.serviceWorker
    .register(new URL('service_worker.js', PAGE_DIR))
    .catch((e: unknown) => console.log('[coi] service worker registration failed:', e))

  if (navigator.serviceWorker.controller) {
    sessionStorage.removeItem(FLAG)
    return
  }
  // Not controlled — a first visit or a hard reload (both bypass the SW for the navigation, so
  // this page still has the server's require-corp headers). Once the worker is active, reload
  // ONCE so the navigation goes through it; the flag stops a broken worker from reload-looping.
  void navigator.serviceWorker.ready.then(() => {
    if (navigator.serviceWorker.controller) return
    if (sessionStorage.getItem(FLAG)) {
      sessionStorage.removeItem(FLAG)
      console.error('[coi] service worker failed to take control after reload')
    } else {
      sessionStorage.setItem(FLAG, 'true')
      window.location.reload()
    }
  })
}

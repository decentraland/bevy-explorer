// COOP/COEP escape hatch for production. The hosting site (decentraland.zone/bevy-web) serves
// `COEP: require-corp`, which blocks plain <img> loads from catalysts (they send no CORP header).
// The shared service worker at the PACKAGE ROOT (deploy/web/service_worker.js) rewrites
// same-origin responses — including this page's own navigation — to `COEP: credentialless`,
// which allows credential-less no-CORP images while keeping crossOriginIsolated for the engine.
// The engine page registers the SAME script (engine/main.js), so one registration scopes both.
//
// Dev is a no-op: Vite already serves credentialless headers directly and doesn't host the SW.
//
// Mirrors the engine's main.js dance: register on load; if a registration exists but no worker
// controls the page yet (first visit — headers only apply to SW-served navigations), reload ONCE,
// guarded by a sessionStorage flag so a broken SW can't reload-loop.
import { PAGE_DIR } from './publicUrl'

export function registerCoiServiceWorker(): void {
  if (!import.meta.env.PROD || !('serviceWorker' in navigator)) return

  window.addEventListener('load', () => {
    navigator.serviceWorker
      .register(new URL('service_worker.js', PAGE_DIR))
      .catch((e: unknown) => console.log('[coi] service worker registration failed:', e))
  })

  if (navigator.serviceWorker.controller) {
    sessionStorage.removeItem('sw_reloaded')
    return
  }
  void navigator.serviceWorker.getRegistration().then((registration) => {
    if (!registration) return
    if (sessionStorage.getItem('sw_reloaded')) {
      sessionStorage.removeItem('sw_reloaded')
      console.error('[coi] service worker failed to take control after reload')
    } else {
      sessionStorage.setItem('sw_reloaded', 'true')
      window.location.reload()
    }
  })
}

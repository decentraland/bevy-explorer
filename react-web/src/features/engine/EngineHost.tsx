// Hosts the engine IN THIS DOCUMENT (Approach A — no iframe, no old boot page): renders the
// canvas the wasm attaches to (#mygame-canvas) at z-0 behind the HUD, injects the engine's boot
// module (deploy/web/engine/boot.js) plus its CDN media libs, and hands the EngineRpc this very
// window — all the engine globals (__bevyLaunch, __bevyLoadProgress, __bevyPanic, window.engine)
// live here now. Same-origin/same-document, so no postMessage or contentWindow plumbing.

import { useEffect } from 'react'
import type { EngineRpc } from '../../engine/engineRpc'
import { bridgeChannelName } from '../../engine/protocol'
import { bootMode } from '../../lib/bootMode'
import { PAGE_DIR } from '../../lib/publicUrl'
// Moved to lib/systemScene.ts, which also decides whether a link is allowed to override it.
import { SYSTEM_SCENE } from '../../lib/systemScene'

// The main (Genesis City) realm. Exported: parcel launches pass it EXPLICITLY so a ?realm
// override — possibly an invalid world — never leaks into a Places pick (always a Genesis
// coordinate).
export const DEFAULT_REALM = 'https://realm-provider-ea.decentraland.org/main'

// Engine media libs the wasm expects as globals (LivekitClient, Hls) — loaded from CDNs like the
// old boot page did.
const CDN_LIBS = [
  'https://cdn.jsdelivr.net/npm/livekit-client/dist/livekit-client.umd.min.js',
  'https://cdn.jsdelivr.net/npm/hls.js@1'
]

let injected = false
function injectEngine(): void {
  // Seed window.__bridgeSession BEFORE the engine boots: engine.js forwards it to the scene
  // sandbox, which then scopes the bridge BroadcastChannel to this tab (issue #1089). The suffix
  // is opt-in per host page — engine.js never generates one — so this call is what turns it on.
  // Above the guard so a host page that pre-set __bevyBootConfig (and bails below) still seeds
  // the id the page-side channel constructors will use.
  bridgeChannelName()

  // Once per page — the engine is a singleton (StrictMode remounts and HMR must not double-boot).
  if (injected || window.__bevyBootConfig != null) return
  injected = true

  const params = new URLSearchParams(location.search)
  // pkg/ fetch base: the versioned CDN in prod builds (BASE_URL), the served engine dir otherwise.
  window.PUBLIC_URL = new URL('engine', new URL(import.meta.env.BASE_URL, PAGE_DIR)).href
  window.__bevyBootConfig = {
    systemScene: bootMode().systemScene ?? SYSTEM_SCENE,
    portables: params.get('portables') ?? undefined,
    preview: params.has('preview')
  }

  for (const src of CDN_LIBS) {
    const s = document.createElement('script')
    s.src = src
    s.crossOrigin = 'anonymous'
    document.head.appendChild(s)
  }
  const boot = document.createElement('script')
  boot.type = 'module'
  boot.src = new URL('engine/boot.js', PAGE_DIR).href
  document.head.appendChild(boot)
}

declare global {
  interface Window {
    PUBLIC_URL?: string
    __bevyBootConfig?: { systemScene?: string; portables?: string; preview?: boolean }
  }
}

export function EngineHost({ rpc }: { rpc: EngineRpc }): React.JSX.Element {
  useEffect(() => {
    injectEngine()
    rpc.setWindow(window)
    return () => rpc.setWindow(null)
  }, [rpc])

  return (
    <div
      id="canvas-parent"
      style={{ position: 'fixed', inset: 0, width: '100%', height: '100%', zIndex: 0 }}
    >
      {/* The wasm attaches to this canvas (bevy's canvas selector). */}
      <canvas id="mygame-canvas" style={{ width: '100%', height: '100%', display: 'block' }} />
      {/* gpu_cache.js flips this to flex while shaders compile; engineRpc.renderBusy() reads it
          to hold the loading overlay until the first real frame. No visual — React owns the UI. */}
      <div id="shader-compiling" style={{ display: 'none' }} />
    </div>
  )
}

// Hosts the engine in a same-origin iframe (served by the Vite middleware under
// /engine/) and hands its window to the EngineRpc once loaded. Sits behind the
// React UI overlay; becomes visible when the login overlay unmounts.

import { useEffect, useRef } from 'react'
import type { EngineRpc } from '../../engine/engineRpc'

const REALM = 'https://realm-provider-ea.decentraland.org/main'

// Our super-user bridge scene, served live by `sdk-commands start` (like
// dcl-editor's editor scene). It relays the scene-loading stream + player-ready
// over BroadcastChannel and renders no UI. The scene SOURCE is cross-origin, but
// it executes in the engine's same-origin worker, so its channel reaches the page.
const SYSTEM_SCENE = 'http://localhost:8100'

// Trailing slash matters: the engine derives its service-worker scope + worker
// paths from location.pathname, so it must boot at /engine/ not /engine/index.html.
// hideLoader=1 suppresses the engine's built-in loading UI — React renders the only loader.
const ENGINE_SRC =
  `/engine/?initialRealm=${encodeURIComponent(REALM)}` +
  `&position=0,0&systemScene=${encodeURIComponent(SYSTEM_SCENE)}&hideLoader=1`

export function EngineHost({ rpc }: { rpc: EngineRpc }): React.JSX.Element {
  const ref = useRef<HTMLIFrameElement>(null)

  useEffect(() => {
    const iframe = ref.current
    if (!iframe) return
    const attach = (): void => rpc.setWindow(iframe.contentWindow)
    iframe.addEventListener('load', attach)
    if (iframe.contentWindow) attach()
    return () => {
      iframe.removeEventListener('load', attach)
      rpc.setWindow(null)
    }
  }, [rpc])

  return (
    <iframe
      ref={ref}
      src={ENGINE_SRC}
      title="Decentraland engine"
      allow="autoplay; fullscreen; xr-spatial-tracking; microphone; clipboard-write"
      style={{
        position: 'fixed',
        inset: 0,
        width: '100%',
        height: '100%',
        border: 'none',
        zIndex: 0
      }}
    />
  )
}

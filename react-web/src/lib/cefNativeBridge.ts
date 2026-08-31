// NATIVE via CEF (feature react-hud-cef): the HUD runs in a CEF offscreen webview composited by
// the engine, and window.cef.{emit,listen} (injected by cef_offscreen's render process at
// document start) is the transport. This shim rides the page's Envelopes over it — the app
// itself stays transport-agnostic.
//
// No-op when window.cef is absent (web).

import { bridgeChannelName } from '../engine/protocol'

interface CefApi {
  emit: (payload: unknown) => void
  listen: (id: string, cb: (payload: unknown) => void) => void
}

export function installCefNativeBridge(): void {
  const cef = (window as Window & { cef?: CefApi }).cef
  if (!cef?.emit || !cef?.listen) return

  const ch = new BroadcastChannel(bridgeChannelName())
  // page -> engine: only scene-bound Envelopes cross the boundary
  ch.onmessage = (e) => {
    const env = e.data as { to?: string } | undefined
    if (env?.to === 'scene') cef.emit(env)
  }
  // engine -> page: the native relay emits full Envelopes as JSON strings
  cef.listen('bridge', (payload) => {
    try {
      ch.postMessage(typeof payload === 'string' ? JSON.parse(payload) : payload)
    } catch {
      /* malformed envelope; drop */
    }
  })
  // (HUD focus — including text focus — now flows through the bridge scene as a 'uiFocus'
  // message on every platform; see useEngineSession. No engine-addressed messages remain.)
  // Engine fps for the perf overlay and logical height for --ui-scale (see useFps/useHudScale).
  cef.listen('engineFps', (v) => {
    ;(window as Window & { __nativeEngineFps?: number }).__nativeEngineFps = Number(v)
  })
  cef.listen('uiHeight', (v) => {
    ;(window as Window & { __nativeUiHeight?: number }).__nativeUiHeight = Number(v)
    window.dispatchEvent(new Event('resize'))
  })
  // Engine-side text focus (scene textinput / engine text box): keys forward to this page
  // unconditionally, so the systemAction dispatcher needs the same don't-treat-keys-as-shortcuts
  // signal boot.js provides on web (see push_text_focus in src/react_hud_cef.rs).
  cef.listen('engineTextFocus', (v) => {
    ;(window as Window & { __engineTextFocus?: boolean }).__engineTextFocus = v === 'true'
  })
}

// NATIVE via CEF (feature react-hud-cef): the HUD runs in a CEF offscreen webview composited by
// the engine, and window.cef.{emit,listen} (injected by cef_offscreen's render process at
// document start) is the transport. This shim rides the page's Envelopes over it — the app
// itself stays transport-agnostic.
//
// No-op when window.cef is absent (web).

import { BRIDGE_CHANNEL } from '../engine/protocol'

interface CefApi {
  emit: (payload: unknown) => void
  listen: (id: string, cb: (payload: unknown) => void) => void
}

export function installCefNativeBridge(): void {
  const cef = (window as Window & { cef?: CefApi }).cef
  if (!cef?.emit || !cef?.listen) return

  const ch = new BroadcastChannel(BRIDGE_CHANNEL)
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
  // Text-focus signal: while a HUD text field is focused, the engine keeps forwarding keys to
  // the page even if the cursor drifts over the world (chat typing isn't cut off mid-word).
  const typing = (t: EventTarget | null): boolean => {
    const el = t as HTMLElement | null
    return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)
  }
  document.addEventListener(
    'focusin',
    (e) => {
      if (typing(e.target)) cef.emit({ to: 'engine', msg: { kind: 'textFocus', focused: true } })
    },
    true
  )
  document.addEventListener(
    'focusout',
    () => {
      setTimeout(() => {
        if (!typing(document.activeElement))
          cef.emit({ to: 'engine', msg: { kind: 'textFocus', focused: false } })
      }, 0)
    },
    true
  )
  // Engine fps for the perf overlay and logical height for --ui-scale (see useFps/useHudScale).
  cef.listen('engineFps', (v) => {
    ;(window as Window & { __nativeEngineFps?: number }).__nativeEngineFps = Number(v)
  })
  cef.listen('uiHeight', (v) => {
    ;(window as Window & { __nativeUiHeight?: number }).__nativeUiHeight = Number(v)
    window.dispatchEvent(new Event('resize'))
  })
  // Engine-side text focus (scene textinput / engine text box): keys forward to this page
  // unconditionally, so useMenuShortcuts needs the same don't-treat-keys-as-shortcuts signal
  // boot.js provides on web (see push_text_focus in src/react_hud_cef.rs).
  cef.listen('engineTextFocus', (v) => {
    ;(window as Window & { __engineTextFocus?: boolean }).__engineTextFocus = v === 'true'
  })
}

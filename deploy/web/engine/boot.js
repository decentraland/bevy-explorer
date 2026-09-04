// Same-document engine bootstrap — everything the old boot page (index.html + ui.js + main.js)
// provided, minus its DOM. The React app (react-web) renders the only UI: it injects this module,
// reads the globals below, and drives the launch.
//
// Contract with the host page (react-web/src/engine/engineRpc.ts):
//   set BEFORE injecting:  window.PUBLIC_URL   — base for pkg/ fetches (versioned CDN in prod)
//                          window.__bevyBootConfig — the engine_run options (keyed by the web
//                            param table: systemScene, portables, preview, the
//                            host-resolved baseDomain and service overrides …) minus
//                            realm/position, forwarded verbatim by __bevyLaunch
//   provided by this module:
//     __bevyLoadProgress / __bevyLoadStep  — weighted boot progress for the login bar
//     __bevyReadyToLaunch / __bevyLaunch(realm?, position?) — deferred engine_run
//     __bevyPanic — readable Rust panic text (the JS throw is a generic "unreachable" trap)
//     __engineHeartbeat / reportEngineError / __rearmCrashWatchdog — crash watchdog plumbing
//     __engineTextFocus — true while an engine-rendered text field holds keyboard focus (the
//       wasm keeps it current via __setEngineTextFocus); HUD hotkeys must not fire while set
//     __onEngineCrash(message, source) — OPTIONAL host callback; the watchdog calls it instead of
//       rendering any overlay (React owns the error UI)
//     window.engine / engine_console_command — the console RPC (built by engine.js post-launch)
//     __defaultRealm() / __serviceUrl(name) / __defaultBaseDomain() — HOST-PROVIDED
//       (react-web/src/lib/baseDomain.ts, defined before this module is injected), for the url
//       sync only: the host's default realm, a service's resolved base url (its ?<name>= override,
//       else composed from the base domain), and the domain a url without ?baseDomain= means
//       here. The engine itself takes the domain and the overrides as engine_run options.
//     __bevyHomeScene() — the persisted home scene { realm, parcel: "x,y" } (realm null = none
//       pinned), for the host's "Skip to Home"; set alongside __bevyReadyToLaunch
import { initEngine, start, applyOptionsToUrlParams, engine_home_scene, gpu_cache_hash, initGpuCache } from './engine.js'

// ---- boot progress (replaces ui.js's DOM loading steps) -----------------------------------------
// Weight of each step in the overall bar (sums to 100). Step ids are read by the React login bar
// (LoadingAndLogin STEP_LABEL) — keep in sync.
const STEP_WEIGHTS = { download: 80, compile: 5, init: 5, workers: 5, gpu: 5 }
const completed = new Set()
let currentStep = null
let currentProgress = 0

function publish() {
  let total = 0
  for (const s of completed) total += STEP_WEIGHTS[s] ?? 0
  if (currentStep != null) total += ((STEP_WEIGHTS[currentStep] ?? 0) * currentProgress) / 100
  window.__bevyLoadProgress = Math.min(100, Math.round(total))
  window.__bevyLoadStep = currentStep
}
// engine.js calls these as bare globals.
window.setLoadingStepActive = (step) => { currentStep = step; currentProgress = 0; publish() }
window.setLoadingStepProgress = (step, pct) => { currentStep = step; currentProgress = pct; publish() }
window.setLoadingStepCompleted = (step) => { completed.add(step); currentStep = null; currentProgress = 0; publish() }
// Old-page helpers engine.js still references — the canvas is always visible under React.
window.showCanvas = () => {}
window.hideHeader = () => {}
publish()

// ---- crash watchdog + panic capture (ported from the old page, UI-less) -------------------------
// Uncaught errors are only recorded as context; the real "is it dead" signal is the heartbeat the
// Rust loop drives every frame. On a confirmed stall the watchdog calls the HOST's callback
// (window.__onEngineCrash) — React shows the error modal; there is no overlay here.
;(function () {
  let shown = false
  // Set when the crash came from the FLOOD detector: there the heartbeat never stopped, so
  // "frames resumed" is not evidence of recovery — only a host dismiss (__rearmCrashWatchdog)
  // clears this latch. Without it the stall watchdog's auto-rearm would clear `shown` ~4s in,
  // the still-running flood would re-trip, and the crash cycle would repeat forever.
  let floodLatched = false
  let lastError = null
  let lastBeat = 0
  let beatsSeen = false
  let beatCount = 0
  const WATCHDOG_MS = 2000
  const HANG_TICKS = 8 // ~16s with no corroborating error
  const ERROR_CONFIRM_TICKS = 2 // ~4s after a recorded error

  // "Alive but rendering-dead" detector. The stall watchdog above only catches a FROZEN engine (the
  // heartbeat stops). A render loop that keeps beating while every frame throws the SAME error — e.g. a
  // wgpu pipeline/attachment mismatch that blanks the screen and spams the console — sails past it, so
  // catch it by the flood: the same fatal-class engine error repeating FATAL_REPEATS times within
  // FATAL_WINDOW_MS ⇒ runtime crash. Scene (UGC) console.error lives in the sandbox WORKER's own
  // console and never reaches this main-thread patch, so this stream is engine-core only. Signatures are
  // whitespace-normalized but otherwise EXACT (numbers/urls kept) so distinct errors — different assets,
  // different coords — never pool into one false flood; only a truly identical per-frame error trips it.
  const FATAL_REPEATS = 30
  const FATAL_WINDOW_MS = 4000
  // Only these engine messages can trip the flood. A healthy session floods the console with benign
  // ERROR lines at exactly this rate — a 404 asset retried per frame by bevy_asset, comms "channel
  // closed" during startup — so an allowlist, not a denylist, is what keeps the modal off a working
  // world. `captured wgpu error` is src/lib.rs's on_uncaptured_error handler: the device-level GPU
  // fault that blanks the screen while the render loop keeps beating.
  const FATAL_SIGNALS = ['captured wgpu error']
  // signature -> { count, since }. Per-signature counters (not a single last-seen key) so an identical
  // per-frame error still floods even when the loop interleaves it with a second error each frame.
  const floodCounts = new Map()

  const nowMs = () => (window.performance?.now ? performance.now() : Date.now())
  const errMessage = (err) => {
    if (!err) return 'Unknown error'
    if (typeof err === 'string') return err
    return err.message || err.stack || String(err)
  }

  function crash(err, source, latch) {
    if (shown) return
    shown = true
    if (latch) floodLatched = true
    const msg = errMessage(err)
    console.error('[crash watchdog]', source || 'error', err)
    try {
      window.__onEngineCrash?.(msg, source || 'error')
    } catch (_) {}
  }

  // Re-arm from a clean slate (host dismissed the modal, or frames resumed).
  function rearm() {
    shown = false
    floodLatched = false
    lastError = null
    lastBeat = nowMs()
    missedTicks = 0
    recoverTicks = 0
    lastBeatCount = beatCount
    floodCounts.clear()
    window.__bevyPanic = undefined
  }
  window.__rearmCrashWatchdog = rearm

  function recordError(err, source) {
    console.error('[engine error]', source || 'error', err)
    lastError = { err, source: source || 'error', at: nowMs() }
  }
  window.reportEngineError = recordError

  // Collapse whitespace only (keep numbers/urls) so "the same error" groups but distinct ones don't.
  const signature = (msg) => String(msg).replace(/\s+/g, ' ').trim().slice(0, 300)

  // Flood detector (see FATAL_REPEATS above): an identical engine-core error repeating every frame is a
  // running-but-dead loop the stall watchdog can't see. Latched via `shown`; rearm() clears the counters.
  function considerFatalRepeat(msg) {
    if (shown) return
    if (!FATAL_SIGNALS.some((s) => msg.indexOf(s) !== -1)) return
    const key = signature(msg)
    const now = nowMs()
    let e = floodCounts.get(key)
    if (!e || now - e.since > FATAL_WINDOW_MS) {
      e = { count: 0, since: now }
      floodCounts.set(key, e)
    }
    if (++e.count >= FATAL_REPEATS) crash(msg, 'runtime', true)
    // Bound memory: drop signatures whose window has lapsed once the map grows.
    if (floodCounts.size > 64) {
      for (const [k, v] of floodCounts) if (now - v.since > FATAL_WINDOW_MS) floodCounts.delete(k)
    }
  }

  // Capture Rust panic text: the wasm panic hook prints "panicked at …" via console.error (the only
  // engine path that uses that channel — wasm-bindgen's __wbg_error), while the throw that surfaces
  // is a generic trap — stash the readable message for the host.
  const origConsoleError = console.error.bind(console)
  console.error = function () {
    try {
      const first = arguments[0]
      if (typeof first === 'string' && first.indexOf('panicked at') !== -1) {
        window.__bevyPanic = { message: String(first), at: nowMs() }
        recordError(first, 'panic')
      }
    } catch (_) {}
    return origConsoleError.apply(console, arguments)
  }

  // Engine LOG lines — including every error!() — come through console.LOG, not console.error:
  // bevy_log's wasm path (LogPlugin with no custom_layer) is tracing-wasm, whose only console
  // binding is `log`. Each event is a 4-arg styled call whose first argument starts with the level
  // ("%cERROR%c src/lib.rs:505 …"). So the flood detector has to listen here, or the wgpu storm this
  // watchdog exists for never reaches it. FATAL_SIGNALS keeps the benign ERROR spam out.
  const origConsoleLog = console.log.bind(console)
  console.log = function () {
    try {
      const first = arguments[0]
      if (typeof first === 'string') considerFatalRepeat(first)
    } catch (_) {}
    return origConsoleLog.apply(console, arguments)
  }

  window.__engineHeartbeat = () => {
    lastBeat = nowMs()
    beatsSeen = true
    beatCount++
  }

  window.addEventListener('error', (e) => recordError(e.error || e.message, 'error'))
  window.addEventListener('unhandledrejection', (e) => recordError(e.reason, 'unhandledrejection'))

  // Tick-counting (not wall-clock) so throttled background tabs never trip it — see the Rust
  // heartbeat in src/web.rs.
  let lastBeatCount = 0
  let missedTicks = 0
  let recoverTicks = 0
  setInterval(() => {
    if (!beatsSeen || document.hidden) { missedTicks = 0; recoverTicks = 0; return }
    const advanced = beatCount !== lastBeatCount
    lastBeatCount = beatCount

    if (shown) {
      // Frames back for two consecutive ticks → transient stall, re-arm. Never for a flood crash:
      // there the frames never stopped (that's the whole point), so this would clear the latch and
      // let the ongoing flood re-trip every few seconds. Only a host dismiss re-arms that one.
      if (floodLatched) return
      recoverTicks = advanced ? recoverTicks + 1 : 0
      if (recoverTicks >= 2) rearm()
      return
    }
    if (advanced) { missedTicks = 0; return }
    missedTicks++
    const corroborated = lastError && lastError.at >= lastBeat - 1000
    if (missedTicks < (corroborated ? ERROR_CONFIRM_TICKS : HANG_TICKS)) return
    if (lastError) crash(lastError.err, lastError.source)
    else
      crash(
        `The engine stopped responding (no frames for ~${Math.round((missedTicks * WATCHDOG_MS) / 1000)}s). ` +
          'It may have run out of memory, or a worker thread crashed and deadlocked the main thread.',
        'watchdog'
      )
  }, WATCHDOG_MS)
})()

// Host-provided config (set before this module is injected).
const config = window.__bevyBootConfig ?? {}

// ---- engine text focus (CALLED BY THE WASM — src/web.rs update_text_focus) -----------------------
// True while an engine-rendered text field (e.g. a scene textinput) holds keyboard focus. Those
// fields live on the canvas, so DOM focus checks can't see them — the HUD's systemAction
// dispatcher reads this flag instead to leave keys alone while the user is typing.
window.__engineTextFocus = false
window.__setEngineTextFocus = (focused) => {
  window.__engineTextFocus = !!focused
}

// ---- keyboard forwarding to the engine -----------------------------------------------------------
// winit attaches its keyboard listeners to the CANVAS element, so the engine only hears keys while
// the canvas holds DOM focus — click any HUD control and movement + every engine binding (which
// also drives the HUD hotkeys via the system-action stream) would go dead. Re-dispatch window-level
// key events to the canvas when focus sits elsewhere. Bubble phase, so a handler that
// stopPropagation()s (chat input, widgets that own a key locally) still withholds keys from the
// engine; text inputs are skipped so typing never moves the avatar. Tab is left alone off-canvas —
// there it is focus navigation, not a game key. The re-dispatched clone bubbles back here with the
// canvas as target, which the target check drops (no loop).
const forwardKeyToEngine = (e) => {
  const canvas = document.getElementById('mygame-canvas')
  if (!canvas || e.target === canvas) return
  const t = e.target
  if (t instanceof HTMLElement && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return
  if (e.code === 'Tab') return
  canvas.dispatchEvent(new KeyboardEvent(e.type, e))
}
window.addEventListener('keydown', forwardKeyToEngine)
window.addEventListener('keyup', forwardKeyToEngine)

// Wheel likewise: winit's wheel listener also lives on the canvas, so wheel over any HUD element
// would never reach the engine (no camera zoom while the cursor rests on the sidebar). Forward a
// clone; the original still scrolls whatever it was over. Over a SCROLLABLE panel the page
// declares `scroll` focus and the engine stands the Scroll-bound axes down instead — so the wheel
// scrolls chat without zooming, and zooms everywhere else. Same no-loop target check as keys.
const forwardWheelToEngine = (e) => {
  const canvas = document.getElementById('mygame-canvas')
  if (!canvas || e.target === canvas) return
  canvas.dispatchEvent(new WheelEvent('wheel', e))
}
window.addEventListener('wheel', forwardWheelToEngine)

// ---- URL sync (CALLED BY THE WASM — src/web.rs set_url_params) -----------------------------------
// Keeps the browser URL in step with the engine so a reload/share lands back in the same state.
// The wasm sends its CURRENT launch options — the engine_run options object with the live realm,
// position, ui scene, portables and mode swapped in — so every param given at launch is echoed
// back, not just the ones this file knows about. Defaults are omitted so the canonical entry URL
// stays clean; unknown (HUD-only) params are preserved.
// The default realm and service urls are the HUD's call (see the contract above): a ?<service>=
// entry param, else composed from the base domain.
// The engine connects to the EXPANDED world url (ipfs map_realm_name turns `name.dcl.eth` into
// worlds-content-server…/world/name.dcl.eth) and echoes that back here. Reverse it so the address
// bar keeps the short name the user typed; a reload re-expands it the same way.
const WORLDS_PREFIX = `${window.__serviceUrl('worldsServer')}/world/`
// captured from the ENTRY url (later syncs rewrite location.search)
const explicitSystemScene = new URLSearchParams(window.location.search).has('systemScene')
// The values that mean "default" and are dropped from the url — the defaults only this page
// knows (the engine already omits its own, e.g. the default portables). systemScene: null from the
// engine = NO ui scene (systemScene=none); an explicit ?systemScene= boot override stays across
// reloads, only the host's default scene is omitted. baseDomain: the host always passes the
// resolved domain, so it is a url param only when it differs from what this origin derives.
const urlValue = (name, value) => {
  switch (name) {
    case 'realm':
      if (typeof value === 'string' && value.startsWith(WORLDS_PREFIX)) value = value.slice(WORLDS_PREFIX.length)
      return value === window.__defaultRealm() ? null : value
    case 'baseDomain':
      return value === window.__defaultBaseDomain() ? null : value
    case 'systemScene':
      value = value ?? 'none'
      return explicitSystemScene || value !== (config.systemScene ?? 'none') ? value : null
    default:
      return value
  }
}
window.set_url_params = (optionsJson) => {
  try {
    const urlParams = new URLSearchParams(window.location.search)
    const options = Object.fromEntries(Object.entries(JSON.parse(optionsJson)).map(([name, raw]) => [name, urlValue(name, raw)]))
    applyOptionsToUrlParams(urlParams, options)
    history.replaceState(null, '', window.location.pathname + '?' + urlParams.toString())
  } catch (e) {
    console.log(`set url params failed: ${e}`)
  }
}

// ---- F11 fullscreen (ported from the old page) ---------------------------------------------------
window.addEventListener('keydown', (event) => {
  if (event.key === 'F11' || event.code === 'F11') {
    event.preventDefault()
    const canvas = document.getElementById('mygame-canvas')
    if (canvas) {
      if (!document.fullscreenElement) canvas.requestFullscreen?.()
      else document.exitFullscreen?.()
    }
  }
})

// ---- boot: compile WASM + warm the GPU cache, then wait for the host to launch -------------------
initEngine()
  .then(() => {
    window.setLoadingStepActive('gpu')
    return initGpuCache(gpu_cache_hash())
  })
  .then(() => {
    window.setLoadingStepCompleted('gpu')
    // Deferred launch: the host calls this once the user picks a destination — avoiding a wasted
    // default-realm load. One engine per page (see start()'s __bevyStarted guard).
    window.__bevyLaunch = (realm, position) => start({ ...config, realm, position })
    // The persisted home scene ({ realm (null = none pinned), parcel: "x,y" }), valid once
    // engine_init has loaded the config — the host's places picker targets it from "Skip to
    // Home" before launching.
    window.__bevyHomeScene = () => { try { return JSON.parse(engine_home_scene()) } catch { return null } }
    window.__bevyReadyToLaunch = true
  })
  .catch((e) => {
    console.error('[boot] engine init failed', e)
    window.reportEngineError?.(e, 'boot')
  })

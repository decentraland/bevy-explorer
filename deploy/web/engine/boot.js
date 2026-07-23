// Same-document engine bootstrap — everything the old boot page (index.html + ui.js + main.js)
// provided, minus its DOM. The React app (react-web) renders the only UI: it injects this module,
// reads the globals below, and drives the launch.
//
// Contract with the host page (react-web/src/engine/engineRpc.ts):
//   set BEFORE injecting:  window.PUBLIC_URL   — base for pkg/ fetches (versioned CDN in prod)
//                          window.__bevyBootConfig = { systemScene, portables, preview }
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
import { initEngine, start, gpu_cache_hash, initGpuCache } from './engine.js'

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
  // catch it by the flood: the identical engine-core console.error repeating FATAL_REPEATS times within
  // FATAL_WINDOW_MS ⇒ runtime crash. Scene (UGC) console.error lives in the sandbox WORKER's own
  // console and never reaches this main-thread patch, so this stream is engine-core only. Signatures are
  // whitespace-normalized but otherwise EXACT (numbers/urls kept) so distinct errors — different assets,
  // different coords — never pool into one false flood; only a truly identical per-frame error trips it.
  const FATAL_REPEATS = 30
  const FATAL_WINDOW_MS = 4000
  // Engine-core errors that can repeat identically without being fatal — never trip the flood on these.
  const BENIGN_REPEATS = ['Message too large']
  // signature -> { count, since }. Per-signature counters (not a single last-seen key) so an identical
  // per-frame error still floods even when the loop interleaves it with a second error each frame.
  const floodCounts = new Map()

  const nowMs = () => (window.performance?.now ? performance.now() : Date.now())
  const errMessage = (err) => {
    if (!err) return 'Unknown error'
    if (typeof err === 'string') return err
    return err.message || err.stack || String(err)
  }

  function crash(err, source) {
    if (shown) return
    shown = true
    const msg = errMessage(err)
    console.error('[crash watchdog]', source || 'error', err)
    try {
      window.__onEngineCrash?.(msg, source || 'error')
    } catch (_) {}
  }

  // Re-arm from a clean slate (host dismissed the modal, or frames resumed).
  function rearm() {
    shown = false
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
    if (BENIGN_REPEATS.some((b) => msg.indexOf(b) !== -1)) return
    const key = signature(msg)
    const now = nowMs()
    let e = floodCounts.get(key)
    if (!e || now - e.since > FATAL_WINDOW_MS) {
      e = { count: 0, since: now }
      floodCounts.set(key, e)
    }
    if (++e.count >= FATAL_REPEATS) crash(msg, 'runtime')
    // Bound memory: drop signatures whose window has lapsed once the map grows.
    if (floodCounts.size > 64) {
      for (const [k, v] of floodCounts) if (now - v.since > FATAL_WINDOW_MS) floodCounts.delete(k)
    }
  }

  // Capture Rust panic text: the wasm panic hook prints "panicked at …" via console.error, while
  // the throw that surfaces is a generic trap — stash the readable message for the host.
  const origConsoleError = console.error.bind(console)
  console.error = function () {
    try {
      const first = arguments[0]
      if (typeof first === 'string' && first.indexOf('panicked at') !== -1) {
        window.__bevyPanic = { message: String(first), at: nowMs() }
        recordError(first, 'panic')
      }
      // Engine-core log lines arrive as a single formatted string (wasm __wbg_error → console.error).
      // Feed them to the flood detector so a per-frame error storm (blank-screen render crash) becomes
      // a runtime crash modal instead of an endless silent spam. Skip our OWN watchdog logging
      // (recordError/crash re-enter this patch) so their constant prefixes can't self-trip the flood.
      if (
        typeof first === 'string' &&
        first.indexOf('[engine error]') !== 0 &&
        first.indexOf('[crash watchdog]') !== 0
      ) {
        considerFatalRepeat(first)
      }
    } catch (_) {}
    return origConsoleError.apply(console, arguments)
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
      // Frames back for two consecutive ticks → transient stall, re-arm.
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
// fields live on the canvas, so DOM focus checks can't see them — HUD hotkey handlers read this
// flag instead (useMenuShortcuts) to leave keys alone while the user is typing.
window.__engineTextFocus = false
window.__setEngineTextFocus = (focused) => {
  window.__engineTextFocus = !!focused
}

// ---- URL sync (CALLED BY THE WASM — src/web.rs set_url_params) -----------------------------------
// Keeps the browser URL in step with the player's realm/position so a reload/share lands back at
// the same place. Defaults are omitted so the canonical entry URL stays clean.
const DEFAULT_SERVER = 'https://realm-provider-ea.decentraland.org/main'
const DEFAULT_PORTABLES = 'basiccontroller.dcl.eth'
// The engine connects to the EXPANDED world url (ipfs map_realm_name turns `name.dcl.eth` into
// worlds-content-server…/world/name.dcl.eth) and echoes that back here. Reverse it so the address
// bar keeps the short name the user typed; a reload re-expands it the same way.
const WORLDS_PREFIX = 'https://worlds-content-server.decentraland.org/world/'
// captured from the ENTRY url (later syncs rewrite location.search)
const explicitSystemScene = new URLSearchParams(window.location.search).has('systemScene')
window.set_url_params = (position, server, system_scene, portables, preview) => {
  try {
    if (server.startsWith(WORLDS_PREFIX)) server = server.slice(WORLDS_PREFIX.length)
    const urlParams = new URLSearchParams(window.location.search)
    // absent when the realm won't honour a position (worlds spawn at their base scene)
    if (position != null) urlParams.set('position', position)
    else urlParams.delete('position')
    if (server !== DEFAULT_SERVER) urlParams.set('realm', server)
    else urlParams.delete('realm')
    // An explicit ?systemScene= boot override stays in the URL across reloads (null from the
    // engine = NO ui scene running, i.e. systemScene=none); only the default scene is omitted
    // to keep the canonical entry URL clean.
    if (explicitSystemScene || system_scene !== (config.systemScene ?? '')) urlParams.set('systemScene', system_scene ?? 'none')
    else urlParams.delete('systemScene')
    if (portables !== DEFAULT_PORTABLES) urlParams.set('portables', portables)
    else urlParams.delete('portables')
    if (preview) urlParams.set('preview', true)
    else urlParams.delete('preview')
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
    window.__bevyLaunch = (realm, position) => start({ realm, position, systemScene: config.systemScene, portables: config.portables, preview: config.preview })
    window.__bevyReadyToLaunch = true
  })
  .catch((e) => {
    console.error('[boot] engine init failed', e)
    window.reportEngineError?.(e, 'boot')
  })

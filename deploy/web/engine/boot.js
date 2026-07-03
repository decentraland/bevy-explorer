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
    window.__bevyPanic = undefined
  }
  window.__rearmCrashWatchdog = rearm

  function recordError(err, source) {
    console.error('[engine error]', source || 'error', err)
    lastError = { err, source: source || 'error', at: nowMs() }
  }
  window.reportEngineError = recordError

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

// ---- URL sync (CALLED BY THE WASM — src/web.rs set_url_params) -----------------------------------
// Keeps the browser URL in step with the player's realm/position so a reload/share lands back at
// the same place. Defaults are omitted so the canonical entry URL stays clean.
const DEFAULT_SERVER = 'https://realm-provider-ea.decentraland.org/main'
const DEFAULT_PORTABLES = 'basiccontroller.dcl.eth'
window.set_url_params = (position, server, system_scene, portables, preview) => {
  try {
    const urlParams = new URLSearchParams(window.location.search)
    // absent when the realm won't honour a position (worlds spawn at their base scene)
    if (position != null) urlParams.set('position', position)
    else urlParams.delete('position')
    if (server !== DEFAULT_SERVER) urlParams.set('realm', server)
    else urlParams.delete('realm')
    if (system_scene !== (config.systemScene ?? '')) urlParams.set('systemScene', system_scene)
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

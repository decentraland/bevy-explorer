// Thin wrapper over the engine's console-command RPC seam.
//
// Upstream bevy-explorer exposes `window.engine_console_command(line)` on the
// engine document (src/web.rs, attached in deploy/web/engine.js). The engine shares
// this document (EngineHost "Approach A", no iframe), so the target is just `window`;
// which window to point at is kept behind here so callers don't care.

type EngineWindow = Window & {
  engine_console_command?: (line: string) => Promise<string>
  __bevyReadyToLaunch?: boolean
  __bevyLaunch?: (realm?: string, position?: string) => void
  __bevyLoadProgress?: number
  __bevyLoadStep?: string | null
  __bevyPanic?: { message: string }
  __rearmCrashWatchdog?: () => void
}

export class EngineRpc {
  private win: EngineWindow | null = null

  setWindow(win: Window | null): void {
    this.win = win as EngineWindow | null
  }

  /** True once the WASM is compiled + GPU cache warm (manualParams mode) — ready to be launched. */
  readyToLaunch(): boolean {
    return this.win?.__bevyReadyToLaunch === true
  }

  /** Overall weighted boot progress (0–100) from the engine loader; 0 if the engine isn't ready yet. */
  loadProgress(): number {
    return this.win?.__bevyLoadProgress ?? 0
  }

  /** Current boot step id ('download'|'compile'|'init'|'workers'|'gpu') or null. */
  loadStep(): string | null {
    return this.win?.__bevyLoadStep ?? null
  }

  /** Last Rust panic text captured from the engine console (manualParams boot), or null. The throw
   *  that reaches us is only a generic "unreachable" trap; the readable message is stashed here. */
  enginePanic(): { message: string } | null {
    return this.win?.__bevyPanic ?? null
  }

  /** Drop the stashed panic once consumed — it's set on every "panicked at" log and never cleared by
   *  the engine, so a later read (a fresh launch throw, or the boot poll) would surface a stale one. */
  clearEnginePanic(): void {
    if (this.win) this.win.__bevyPanic = undefined
  }

  /** Re-arm the engine's crash watchdog (resets its `shown` flag) after the host dismisses a runtime
   *  crash — otherwise a second genuine crash hits the watchdog's `if (shown) return` and is swallowed. */
  rearmCrashWatchdog(): void {
    this.win?.__rearmCrashWatchdog?.()
  }

  /** Boot the bevy app at a realm/position (only valid in manualParams mode, after readyToLaunch). */
  launch(realm?: string, position?: string): void {
    this.win?.__bevyLaunch?.(realm, position)
  }

  ready(): boolean {
    return typeof this.win?.engine_console_command === 'function'
  }

  async waitReady(timeoutMs = 90_000): Promise<void> {
    const start = performance.now()
    while (!this.ready()) {
      if (performance.now() - start > timeoutMs) {
        throw new Error('engine RPC did not become ready in time')
      }
      await new Promise((r) => setTimeout(r, 150))
    }
  }

  /** Run a console command line (e.g. "/login_guest") and return its reply. */
  async command(line: string): Promise<string> {
    await this.waitReady()
    return this.win!.engine_console_command!(line)
  }

  /** True while the engine is still compiling shaders for the current scene — i.e. revealing the
   *  world now would show black/untextured models. Read from the engine doc's `#shader-compiling`
   *  indicator (same document). Best-effort: false when the window/element isn't reachable. */
  renderBusy(): boolean {
    try {
      const w = this.win as (Window & typeof globalThis) | null
      const el = w?.document.getElementById('shader-compiling')
      return el != null && w!.getComputedStyle(el).display !== 'none'
    } catch {
      return false
    }
  }
}

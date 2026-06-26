// Thin wrapper over the engine's console-command RPC seam.
//
// Upstream bevy-explorer exposes `window.engine_console_command(line)` on the
// engine document (src/web.rs, attached in deploy/web/engine.js). The engine runs
// in a same-origin iframe, so we reach it via `iframe.contentWindow`; which window
// to point at is kept behind here so callers don't care.

export class EngineRpc {
  private win: (Window & { engine_console_command?: (line: string) => Promise<string> }) | null =
    null

  setWindow(win: Window | null): void {
    this.win = win as typeof this.win
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
   *  indicator (same-origin iframe). Best-effort: false when the window/element isn't reachable. */
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

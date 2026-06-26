// Thin wrapper over the engine's console-command RPC seam.
//
// Upstream bevy-explorer exposes `window.engine_console_command(line)` on the
// engine document (src/web.rs, attached in deploy/web/engine.js). When the engine
// runs in a same-origin iframe we reach it via `iframe.contentWindow`; the only
// thing that differs between same-document and iframe hosting is which window we
// point at — keep that behind here.

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
}

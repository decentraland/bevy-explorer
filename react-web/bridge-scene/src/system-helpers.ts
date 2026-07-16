// Small helpers for per-frame bridge systems (ctx.push / engine.addSystem): a dt-based throttle
// and a single-flight guard for async work polled from a system.
// The dt-accumulator throttle is otherwise re-implemented inline across most domains
// (chat, world, friends, project, nametags, avatarPreview) — see backlog to migrate them.

/** Wrap `fn` so it runs at most once per `interval` seconds. Feed it the frame `dt` each tick. */
export function throttleByDt(interval: number, fn: () => void): (dt: number) => void {
  let cooldown = 0
  return (dt) => {
    cooldown -= dt
    if (cooldown > 0) return
    cooldown = interval
    fn()
  }
}

/**
 * Wrap an async `fn` so overlapping calls are skipped while one is in flight (single-flight).
 * Keeps the "only one at a time" guarantee local instead of relying on the engine RPC channel
 * being FIFO. `fn` should handle its own errors; the in-flight flag clears either way.
 */
export function singleFlight(fn: () => Promise<void>): () => void {
  let inFlight = false
  return () => {
    if (inFlight) return
    inFlight = true
    void fn().finally(() => {
      inFlight = false
    })
  }
}

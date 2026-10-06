// On the web the app runs inside the shell page (index.html), in an iframe the browser keeps in a
// process of its own. Whatever only the top-level page can do (the address bar, top-level
// navigation, the service worker) goes through the shell's message handler, and so does anything
// that needs the signed-in identity's key, which only the shell holds (src/shell/main.ts).

export const inShell = window.parent !== window

export function postToShell(message: { type: string; [key: string]: unknown }): void {
  window.parent.postMessage(message, location.origin)
}

let nextId = 0
const pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>()
let listening = false

// Ask the shell for something and wait for its answer: `bevy-shell:request` out, the matching
// `bevy-shell:response` back.
export function shellRequest<T>(method: string, params?: unknown): Promise<T> {
  if (!listening) {
    listening = true
    window.addEventListener('message', (e: MessageEvent) => {
      if (e.source !== window.parent || e.origin !== location.origin) return
      const m = e.data as { type?: string; id?: number; result?: unknown; error?: string } | null
      if (m?.type !== 'bevy-shell:response' || typeof m.id !== 'number') return
      const call = pending.get(m.id)
      if (!call) return
      pending.delete(m.id)
      if (m.error !== undefined) call.reject(new Error(m.error))
      else call.resolve(m.result)
    })
  }
  return new Promise<T>((resolve, reject) => {
    const id = nextId++
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject })
    postToShell({ type: 'bevy-shell:request', id, method, params })
  })
}

// On the web the app runs inside the shell page (index.html), in an iframe the browser keeps in a
// process of its own. Whatever only the top-level page can do (the address bar, top-level
// navigation, the service worker) goes through the shell's message handler, and so does anything
// that needs the signed-in identity's key, which only the shell holds (src/shell/main.ts).
// Except in a credentialless frame (the places pages' scene embed): its storage is a throwaway
// copy with no sign-in key, and its parent isolates it, so the app runs there on its own. The
// guard in app.html makes the same check.

export const inShell = window.parent !== window && !(window as { credentialless?: boolean }).credentialless

export function postToShell(message: { type: string; [key: string]: unknown }): void {
  window.parent.postMessage(message, location.origin)
}

const REQUEST_TIMEOUT_MS = 10_000
let nextId = 0
const pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>()
let listening = false

// Ask the shell for something and wait for its answer: `bevy-shell:request` out, the matching
// `bevy-shell:response` back. The shell answers at once (unless the request says how long it may
// take), so no answer means no shell.
export function shellRequest<T>(method: string, params?: unknown, timeoutMs = REQUEST_TIMEOUT_MS): Promise<T> {
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
    setTimeout(() => {
      if (pending.delete(id)) reject(new Error(`no answer from the shell to ${method}`))
    }, timeoutMs)
  })
}

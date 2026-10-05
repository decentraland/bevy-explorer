// On the web the app runs inside the shell page (index.html), in an iframe the browser keeps in a
// process of its own. Whatever only the top-level page can do (the address bar, top-level
// navigation, the service worker) goes through the shell's message handler.

export const inShell = window.parent !== window

export function postToShell(message: { type: string; [key: string]: unknown }): void {
  window.parent.postMessage(message, location.origin)
}

// Chrome 142+ gates fetches that cross from a public site into the local network behind the
// "Local Network Access" permission (developer.chrome.com/blog/local-network-access):
// unannotated requests to loopback/private hosts are blocked outright. Annotating them with
// `targetAddressSpace` lets Chrome raise its permission prompt ("Apps on device" in 145+) and
// exempts the plain-http target from mixed-content blocking. The engine's wasm fetches
// (reqwest → web-sys) resolve the global `fetch` at call time, so patching window.fetch covers
// the engine and the HUD alike. The case that matters: a deployed build loading a
// `sdk-commands start` preview realm (?preview=true&realm=http://127.0.0.1:PORT).
//
// Known gap: fetches issued from worker threads use the worker's own global fetch, which this
// page-level patch can't reach — engine asset/realm fetches run on the main thread, so preview
// is covered.

type TargetAddressSpace = 'loopback' | 'local'

export interface LnaRequestInit extends RequestInit {
  targetAddressSpace?: TargetAddressSpace
}

const LOOPBACK = /^(localhost|127(\.\d{1,3}){3}|\[::1\])$/i
// RFC1918 + link-local — Chrome's non-loopback "local" address space.
const LOCAL = /^(10|192\.168|172\.(1[6-9]|2\d|3[01])|169\.254)(\.\d{1,3}){1,3}$/

export function targetAddressSpaceOf(url: string): TargetAddressSpace | null {
  let host: string
  try {
    host = new URL(url, location.href).hostname
  } catch {
    return null // unparseable — let fetch produce its own error
  }
  if (LOOPBACK.test(host)) return 'loopback'
  if (LOCAL.test(host) || host.toLowerCase().endsWith('.local')) return 'local'
  return null
}

export function installLocalNetworkFetch(): void {
  const original = window.fetch.bind(window)
  window.fetch = function (input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
    const url = input instanceof Request ? input.url : String(input)
    const space = targetAddressSpaceOf(url)
    if (space == null || (init != null && 'targetAddressSpace' in init)) {
      return original(input, init)
    }
    const annotated: LnaRequestInit = { ...init, targetAddressSpace: space }
    return original(input, annotated)
  }
}

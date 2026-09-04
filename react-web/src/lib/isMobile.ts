// Mobile detection — mirrors the engine bundle's own gate (deploy/web/index.html). The engine needs
// WebGPU + SharedArrayBuffer, which mobile browsers don't provide, so react-web shows a
// download-the-app gate instead of booting the engine (and never downloads the ~105MB WASM).

const MOBILE_RE = /Android|iPhone|iPad|iPod|webOS|BlackBerry|Opera Mini|IEMobile|Windows Phone/i

// iPadOS 13+ reports as desktop Safari ("Macintosh") but is touch-first.
function isIPadOS(): boolean {
  return /Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1
}

export function isMobile(): boolean {
  if (typeof navigator === 'undefined') return false
  return MOBILE_RE.test(navigator.userAgent) || isIPadOS()
}

// Chromium check — mirrors the engine bundle's gate (deploy/web/index.html `isChromiumBased`). The
// engine only runs on Chromium (Chrome/Edge/Brave/Opera all carry "Chrome/" in the UA); Firefox and
// Safari don't, and the engine renders its own "Browser Not Supported" page there — so react-web must
// gate too, or the HUD mounts over that page and the login sits frozen at 0%.
export function isChromiumBased(): boolean {
  if (typeof navigator === 'undefined') return true // fail-open (SSR/tests): never gate without a UA
  return /Chrome\//.test(navigator.userAgent)
}

// The engine gate's "try anyway" escape hatch sets this cookie; honour it so the bypass is shared.
export function hasBypassCookie(): boolean {
  if (typeof document === 'undefined') return false
  return document.cookie.split(';').some((c) => c.trim().startsWith('bypass_browser_check='))
}

export type MobilePlatform = 'ios' | 'android' | 'other'

export function mobilePlatform(): MobilePlatform {
  const ua = navigator.userAgent
  if (/iPad|iPhone|iPod/.test(ua) || isIPadOS()) return 'ios'
  if (/Android/i.test(ua)) return 'android'
  return 'other'
}

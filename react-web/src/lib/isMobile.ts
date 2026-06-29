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

export type MobilePlatform = 'ios' | 'android' | 'other'

export function mobilePlatform(): MobilePlatform {
  const ua = navigator.userAgent
  if (/iPad|iPhone|iPod/.test(ua) || isIPadOS()) return 'ios'
  if (/Android/i.test(ua)) return 'android'
  return 'other'
}

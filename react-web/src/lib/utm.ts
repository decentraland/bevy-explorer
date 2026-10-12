// The utm_source on links out to Decentraland's sites, telling the web build from the desktop app.

import { isNativeHud } from './bootMode'

export function utmSource(): string {
  return isNativeHud() ? 'bevy-desktop-client' : 'bevy-web-client'
}

export function withUtm(url: string): string {
  return `${url}${url.includes('?') ? '&' : '?'}utm_source=${utmSource()}`
}

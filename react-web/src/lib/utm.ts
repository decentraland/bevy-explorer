// The utm_source on links out to Decentraland's sites, telling the web build from the desktop app.

export function utmSource(): string {
  const desktop = typeof __NATIVE_HUD__ !== 'undefined' && __NATIVE_HUD__ && new URLSearchParams(location.search).get('native') === '1'
  return desktop ? 'bevy-desktop-client' : 'bevy-web-client'
}

export function withUtm(url: string): string {
  return `${url}${url.includes('?') ? '&' : '?'}utm_source=${utmSource()}`
}

// The web explorer's GA4 stream ("Explorer", decentraland.org/play), which Google Ads imports its
// conversion from. The tag loads only here, on the first scene, so nothing of Google's loads before.
const MEASUREMENT_ID = 'G-GE8LTKK0VK'
const REPORTED_KEY = 'dcl-in-world-reported'

export function reportInWorld(loginType: string | undefined): void {
  // the stream is the production page's: previews and local runs must not count as conversions
  if (location.hostname !== 'decentraland.org') return
  try {
    if (sessionStorage.getItem(REPORTED_KEY)) return
    sessionStorage.setItem(REPORTED_KEY, '1')
  } catch {
    // storage blocked: once per page load then
  }
  const w = window as Window & { dataLayer?: unknown[] }
  const dataLayer = (w.dataLayer ??= [])
  // gtag.js reads the arguments object itself; an array is ignored
  function gtag(..._args: unknown[]): void {
    dataLayer.push(arguments)
  }
  const script = document.createElement('script')
  script.src = `https://www.googletagmanager.com/gtag/js?id=${MEASUREMENT_ID}`
  script.async = true
  document.head.appendChild(script)
  gtag('js', new Date())
  gtag('config', MEASUREMENT_ID, { send_page_view: false })
  gtag('event', 'web_in_world', loginType ? { login_type: loginType } : {})
}

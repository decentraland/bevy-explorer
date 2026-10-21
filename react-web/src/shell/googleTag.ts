import type { LoginType } from '../features/session/sessionAnalytics'

// The web explorer's GA4 stream ("Explorer", decentraland.org/play), which Google Ads imports its
// conversion from. The tag loads only here, on the first scene, so nothing of Google's loads before.
const MEASUREMENT_ID = 'G-GE8LTKK0VK'
const REPORTED_KEY = 'dcl-in-world-reported'
let reported = false

export function isLoginType(value: unknown): value is LoginType {
  return value === 'guest' || value === 'fully_registered'
}

export function reportInWorld(loginType: LoginType | undefined): void {
  if (reported) return
  // the stream is the production page's: previews and local runs must not count as conversions
  if (location.hostname !== 'decentraland.org') return
  if (new URLSearchParams(location.search).get('preview') === 'true') return
  reported = true
  try {
    if (sessionStorage.getItem(REPORTED_KEY)) return
    sessionStorage.setItem(REPORTED_KEY, '1')
  } catch {
    // storage blocked: the flag above makes it once per page load instead
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
  // the app reports only once the terms are accepted, so consent is known here
  gtag('consent', 'default', { ad_storage: 'granted', analytics_storage: 'granted', ad_user_data: 'granted', ad_personalization: 'granted' })
  gtag('js', new Date())
  gtag('config', MEASUREMENT_ID, { send_page_view: false })
  gtag('event', 'bevy_first_move_in_world', loginType ? { login_type: loginType } : {})
}

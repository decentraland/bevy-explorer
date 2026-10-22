import type { LoginType } from '../features/session/sessionAnalytics'
import { isCampaignParam } from '../lib/analytics'

// The web explorer's GA4 stream ("Explorer", decentraland.org/play), which Google Ads imports its
// conversion from. The tag loads only here, on the first scene, so nothing of Google's loads before.
const MEASUREMENT_ID = 'G-GE8LTKK0VK'
const REPORTED_KEY = 'dcl-in-world-reported'
let reported = false
const CAMPAIGN_KEY = 'dcl-entry-campaign'

// The url as the visitor arrived, with its campaign and click ids: the shell takes them off the
// address bar at load (lib/analytics.ts dropAttributionParams), long before the tag reads them,
// and the sign-in redirect returns without them, so they are kept for the tab session.
function entryCampaign(): string {
  const fromUrl = new URLSearchParams([...new URLSearchParams(location.search)].filter(([name]) => isCampaignParam(name))).toString()
  try {
    if (fromUrl) sessionStorage.setItem(CAMPAIGN_KEY, fromUrl)
    else return sessionStorage.getItem(CAMPAIGN_KEY) ?? ''
  } catch {
    // storage blocked: only this page load's url counts
  }
  return fromUrl
}
const campaign = entryCampaign()
const ENTRY_LOCATION = location.origin + location.pathname + (campaign ? '?' + campaign : '')

export function isLoginType(value: unknown): value is LoginType {
  return value === 'guest' || value === 'fully_registered'
}

/** GA4's own ids for this browser and visit: its BigQuery export's user_pseudo_id and ga_session_id. */
export interface GoogleTagIds {
  clientId: string
  sessionId: string | null
}

/** The shell's side of `bevy-shell:in-world` (shell/main.ts): the conversion, then GA4's ids posted
 *  back to the app as `bevy-shell:google-tag`. */
export function onInWorld(message: Record<string, unknown>, app: Window): void {
  const loginType = isLoginType(message.login_type) ? message.login_type : undefined
  const dclSessionId = typeof message.session_id === 'string' ? message.session_id : undefined
  reportInWorld(loginType, dclSessionId, (ids) =>
    app.postMessage({ type: 'bevy-shell:google-tag', client_id: ids.clientId, session_id: ids.sessionId }, location.origin)
  )
}

/** The conversion, tagged with the Segment session it belongs to (`dcl_session_id`); `onIds` gets
 *  GA4's ids back once gtag.js is up, for the app to send to Segment: the two sides join both ways. */
export function reportInWorld(loginType: LoginType | undefined, dclSessionId?: string, onIds?: (ids: GoogleTagIds) => void): void {
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
  // the property's login_type dimension is user-scoped, valued web2 / web3 / guest; the page only knows
  // guest or signed in, so web never reports web2 and a social or email sign-in counts as web3
  if (loginType) gtag('set', 'user_properties', { login_type: loginType === 'guest' ? 'guest' : 'web3' })
  gtag('config', MEASUREMENT_ID, { send_page_view: false, page_location: ENTRY_LOCATION })
  gtag('event', 'bevy_first_move_in_world', dclSessionId ? { dcl_session_id: dclSessionId } : {})
  // gtag.js answers a get once it has loaded; blocked, it never does and nothing is sent back
  if (onIds) {
    gtag('get', MEASUREMENT_ID, 'client_id', (clientId: unknown) => {
      gtag('get', MEASUREMENT_ID, 'session_id', (sessionId: unknown) => {
        if (typeof clientId === 'string' && clientId) onIds({ clientId, sessionId: sessionId == null || sessionId === '' ? null : String(sessionId) })
      })
    })
  }
}

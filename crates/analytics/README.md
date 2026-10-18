# analytics

The explorer's Segment events, one sender: the engine. Its own events (Performance Metrics,
Explorer Error, Explorer Scene Load Times, Explorer Move To Parcel, System Info Report, Chat
Message Sent, First Move In World, time_spent_in_world; `data_definition.rs`) and, on the web, the page's screens and clicks, which reach it
through the `/track` console command (`page_events.rs`: a base64 JSON array of
`{event, properties, timestamp}`, queued as sent). The web build posts through the first-party
proxy (`api.e.decentraland.org`), which ad blockers leave alone; native posts to Segment directly.

Every event carries the common fields (`dcl_eth_address`, `dcl_is_guest`, `realm`, `position`,
`dcl_renderer_type`, `session_id`, `renderer_version`), plus two the host page hands over at launch
as host-only web params:

- `session_id`: the page mints one per load, so a session's engine and page events join on it.
  Without one (native) the engine mints its own per run.
- `campaign_anon_user_id`: the landing page's Segment anonymous id (the `ajs_anonymous_id` cookie
  the web shell reads, or `anon_user_id` on the link), as the launcher passes it to the desktop
  client. The warehouse joins the session to the web visit on it; nothing else about the
  campaign travels, the visit already has the UTMs.

Nothing leaves before the user has accepted the terms (`AppConfig::terms_accepted()`, set on the
welcome page). Events queue meanwhile (2000, the oldest dropped) and go out on the first tick
after; each carries the time it was queued, so a held event keeps its place. A run that never
shows the terms (preview, a throwaway `--guest` on a fresh install) never sends.

The page side is `react-web/src/lib/analytics.ts` (events and sequences in
`react-web/review.md`, "Analytics").

Two events answer the funnel's in-world questions (`in_world.rs`):

- `First Move In World`: once per login, the first frame in world with the player moving.
- `time_spent_in_world` (`time_spent` seconds, `realm_name`): the desktop client's event, sent in
  chunks of up to 60 s and when the realm or the login changes, so the sum per session is the
  playtime. A page closed mid-chunk loses that chunk.

# analytics

The explorer's Segment events. The web and native builds send the same events, names and
property shapes as the mobile client (godot-explorer), into one Segment source; `dcl_renderer_type`
tells the clients apart: `dao-bevy-web`, `dao-bevy-<os>` (native), `dao-godot-<platform>`
(mobile). Engine events are emitted from Rust; the web page's screen events reach the engine
through the `/track` console command and leave in the same batch.

## Envelope

Every event is a Segment `track` call: `{ type, event, userId, messageId, timestamp, properties }`.
`userId` is the engine's per-install `user_id` from the app config. `messageId` (uuid v4) and
`timestamp` (RFC3339 ms UTC) are stamped when the event is **queued**, never at flush, and
`properties.client_timestamp` repeats the same string; the batch's `sentAt` is the flush time.

`properties` always carries the common fields, with the mobile defaults until the engine knows
better: `dcl_eth_address` (`"unauthenticated"`, set at login), `dcl_is_guest` (false), `realm`
(`"no-realm"`, the realm root with its trailing slash after a realm change), `position`
(`"no-position"`, the parcel `"x,y"` once the player moves), `dcl_renderer_type`, `session_id`,
`renderer_version`, `utc_offset_minutes` (local = utc + offset). The event's own properties are
merged over them.

Batches go to `https://api.segment.io/v1/batch` every 10 s, at most 500 KB each; an event over
32 KB is dropped with an error log. A 5xx or network failure buffers the batch for retry on the
next tick (≤ 100 events, oldest dropped) with the original `messageId`, so Segment deduplicates; a
4xx other than 408/429 is a rejected payload and is not retried. A batch holds at most 200
events, so a backlog goes out over several ticks.

## Engine events (Rust, native and web)

| Event | Properties | When it fires | Mobile |
|---|---|---|---|
| Performance Metrics | samples, total_time, hiccups_in_thousand_frames, hiccups_time, min/max/mean/median_frame_time, p1…p99_frame_time, player_count, used_jsheap_size, memory_usage | every 1000 frames (`frame.rs`); the last three are `-1`, not measured | same |
| Explorer Error | error_type `"Crash"`, error_message (last 31 KB of the previous run's log), error_stack `""` | native only: at UI ready when launched with the previous run's crash file (`system_ui/crash_report.rs`) | same |
| Explorer Scene Load Times | scene_hash, elapsed (s), success | once per scene entity: when it first runs (past tick 6, or frozen) → true; on load failure or a broken scene → false. Elapsed counts from the entity's first frame; a scene despawned before ready is never reported (`scene_runner/scene_load_times.rs`) | same |
| Explorer Move To Parcel | old_parcel (`"no-position"` for the first); `position` overridden with the new parcel | every parcel change of the primary player while in world, the spawn parcel included; out of world (sign-in, lobby, a teleport in flight) nothing, so `position` stays `"no-position"` until the first placement (`primary_player_parcel_position.rs`) | same |
| System Info Report | os_version (`"web"` / `std::env::consts::OS`, e.g. `"macos"`), processor_count, graphics_device_name, graphics_device_vendor (name mapped from the PCI id; `null` on Metal and WebGPU, which report none), graphics_api_version (`"Vulkan 1.3.280"` style), device_support `"not_checked"`; device_brand, device_model, soc_*, processor_type, processor_max_freq_mhz, system_memory_size_mb always `null` | once, as soon as bevy's render adapter info exists (never headless) (`system_info.rs`) | same |
| Chat Message Sent | length, channel (`"private"` \| `"nearby"`), is_command, is_private, is_mention, screen_name `"CHAT"`; community_id omitted | a message submitted in the engine's chat UI, or sent by the HUD through `SystemApi::SendChat` (a wallet address as channel = private) (`system_ui/chat`) | same |
| Friend Request | receiver_id | `SystemApi::SendFriendRequest` issued (`social`) | same |
| Friend Accept | receiver_id, friendship_id omitted | `SystemApi::AcceptFriendRequest` issued | same |
| Unfriend | receiver_id | `SystemApi::DeleteFriend` issued | same |
| Block User | receiver_id, is_friend (in the friends list at that moment) | `SystemApi::BlockUser` issued | same |
| Loading Event | type, loading_id, schema 1, realm_bucket (`world` \| `genesis` \| `other` \| `unknown`), is_world; started: when (`on_explorer_ready` first load per login, `on_changerealm`, `on_teleport`); completed: duration_ms, dismissed_by (`completion` \| `error` when the parcel's scene failed \| `cancelled` on a logout mid-load) | `started` when the loading screen covers the world (signed-in player out of world, not held for the lobby), `completed` when it clears; background streaming never opens an episode (`system_ui/oow.rs`) | same for started/completed; mobile also sends progress, asset_failure, realm_change_failed |
| First Move In World | position (parcel `"x,y"`), realm, is_guest | once per login: the first in-world frame with non-zero horizontal movement (`user_input/avatar_movement.rs`) | web/native only in Segment; mobile sends `first_move_in_world` to Firebase |

The social events fire when the action is issued, whatever the outcome (mobile emits on the
click). `receiver_id` is the lowercase 0x address, like `dcl_eth_address`. A logout resets
`dcl_eth_address`/`dcl_is_guest` to the defaults.

## Page events (react-web, under the web and the native HUD)

`react-web/src/lib/analytics.ts`. Each is stamped at call time with `created_at`, `message_id`
and the page's `session_id`, queued in prefs (`PREF.analyticsQueue`, newest 200 kept), and sent to
the engine as `/track <base64 JSON array>` of at most 50 once the console answers; a send the
console never answered keeps the items for the next `track()`/`flush()`, an answer consumes them.
The engine accepts only these five names and parses each item's properties into the typed event
(`SegmentEvent::from_page`), skipping an item that doesn't fit without dropping the others.

| Event | Properties | When it fires | Mobile |
|---|---|---|---|
| App Opened | start_kind `"cold"`, trigger (`"deeplink"` when the entry url has `realm`/`position`, else `"icon"`), prev_session_id, seconds_since_last_seen (`-1` unknown) | every page load, first thing (`lib/analyticsBoot.ts` from `main.tsx`) | same; mobile also sends `"warm"` on every return from background, the web only on the auth return (below) |
| Install Attribution | referrer (entry query without `?`), utm_source/medium/campaign/content/term, campaign_token (`c`), click_timestamp = install_timestamp (s), google_play_instant false, attribution_source `"web_url"`, referrer_settled true | once per browser profile and origin (`PREF.analyticsAttributed` in the HUD's prefs; mobile: once per install), when the entry url carries any `utm_*` or `c` | same |
| Guest Wallet Creation | outcome, is_new_user (true on success), failure_reason (`http_4xx` \| `http_5xx` \| `timeout` \| `network` \| `sign_message` \| `bad_response`), http_status?, duration_ms | around the thirdweb guest step (`features/auth/guestWalletEvent.ts`) | same |
| Screen Viewed | screen_name, extra_properties (JSON string, omitted when none) | per screen, below | per screen |
| Click Button | button_text, screen_name, extra_properties? | per click, below | per click |

Screens (`Screen Viewed`), from `features/session/useEngineSession.ts` unless noted:

| screen_name | extra_properties | When | Mobile |
|---|---|---|---|
| START | startup_time_ms, platform `"web"` | page boot, after App Opened | same |
| ACCOUNT_HOME | | the sign-in-or-guest screen shows | same (mobile's sign-in / play-as-guest screen) |
| COMEBACK | | the welcome-back panel after a **failed** stored sign-in | same name; mobile shows it to every returning user |
| ACCOUNT_GUEST_CREATE | | after PLAY_GUEST while a persistent guest is created (shell or native) | same |
| AUTH_BROWSER_OPEN | method `"sso"` | right before the SSO redirect | same |
| AUTH_SUCCESS | login_type `"guest"` \| `"fully_registered"` | the login resolves (a stored login reports its stored kind) | same |
| AUTH_ERROR | error | the login rejects | same |
| WELCOME | | the terms page for an existing profile | web-only |
| AVATAR_NAMING | | the terms page for a new profile | same |
| AUTH_DEPLOY_SUCCESS | | the new profile deployed | same |
| LOBBY | | the lobby phase, and each time the in-world lobby opens | web-only |
| PLACES_PICKER | | the picking phase | web-only |
| LOADING_START | position, realm, when `"on_explorer_ready"` \| `"on_teleport"` | entering towards a destination the user picked: a link, a lobby pick, a map or chat teleport (`sessionAnalytics.ts`); the engine streaming on its own is not reported | same |
| LOADING_END | position, status `"Success"` \| `"Failed"` | the world is reached, or the load falls back to the lobby / sign-in, or a travel is refused | same |
| LOADING_TIMEOUT | total_elapsed_seconds 90, timeout_reason `"max_time"` | 90 s after LOADING_START without an end | same |

Clicks (`Click Button`, `button_text` on `screen_name`):

| screen_name | button_text | Mobile |
|---|---|---|
| ACCOUNT_HOME | SIGN_IN, PLAY_GUEST | same |
| COMEBACK | lets_go, use_another_account | same |
| AUTH_ERROR | try_again (retry a failed stored sign-in, continuing with a default profile) | same |
| AUTH_BROWSER_OPEN | cancel (the native sign-in's cancel, with no preceding AUTH_BROWSER_OPEN screen: that one is only emitted on the SSO path, which leaves the page) | same |
| AVATAR_NAMING | LETS_GO, NAME_RANDOMIZE | same |
| WELCOME | LETS_GO | web-only screen |
| LOBBY | JUMP_IN with extra `{kind: landing \| place \| event, place_id \| event_id}`, CUSTOMIZE, NOTIFICATIONS, CLOSE (`features/lobby/LobbyHome.tsx`) | web-only screen (mobile's JUMP_IN sites also carry `place_id`) |

Mobile-only, never sent by the web or native explorer: Scene Locale Requested, review_prompted,
Firebase Init, Attestation Attempt, Attestation Session Cache Loaded, iOS StoreKit Environment,
Request Result, Push Opened, and push-triggered App Opened.

## Consent gate

Nothing leaves the engine until `SegmentConfig::consent` is true. It starts as
`terms_accepted() || preview` (a preview launch never shows the terms and sends from the start);
`SystemApi::AcceptTerms` (`system_ui/login.rs`) sets it when the user accepts, which flushes what
queued up on the next tick. Meanwhile events accumulate in the engine queue (2000, oldest
dropped, logged once). Page events wait in prefs until the console exists, then in that same
queue, so the sign-in screen's events are sent only after the terms are accepted.

## Session ids

`session_id` is minted once per page load (`crypto.randomUUID()`) and handed to the engine at
launch (`LaunchHostOptions.sessionId` → `boot.js` → `--session_id`, delivery `host`, never
echoed into the url), so engine and page events of one page load share it. Every page event
carries the id of the page load that stamped it, as an explicit `session_id` in the `/track`
item, and that overrides the common field: an event queued in prefs by an earlier load (the
sign-in screen before the SSO redirect, a load whose engine never came up) keeps its own id when a
later load drains it. Without a host id (native, a plain `engine_run`) the engine mints a uuid
per run; the native HUD's page events then carry the page's own id, not the engine's.

## The SSO redirect

`redirectToAuth()` reloads the page, but it is one sign-in. Right before it, the page emits
AUTH_BROWSER_OPEN, calls `flush()` and `markAuthRedirect()` writes `{session, at}` to
`PREF.analyticsRedirect`. The flush may or may not finish before the page unloads (usually the
engine is not launched yet, so there is no console at all); what it does not send crosses the
redirect in prefs and the returning load drains it with the original ids. The returning
load's `bootAnalytics` consumes the marker (always, so it never attributes a later load) and,
within 30 min, resumes that session id before any event. Under mobile's 30 s warm floor
(`seconds_since_last_seen` from `PREF.analyticsLastSeen`, written every 30 s and on `pagehide`)
the return emits nothing; past it, App Opened `{start_kind: "warm", trigger: "icon",
prev_session_id: <the same id>, seconds_since_last_seen}`. No second cold App Opened, START or
Install Attribution. Past 30 min the return is an ordinary cold open with a new id.

## Watching events locally

- `?analyticsDebug=1` on the page logs every queued page event as `console.debug('[analytics]',
  event, properties)`, and `send failed, kept <n>` when `/track` fails. `?mock=1` never attaches an
  engine: events only queue.
- Engine side, the `analytics=debug` log filter (`segment_system.rs`) logs each sent batch with
  its full payload, the retry re-queues and the consent flips (`segment consent set to true`).
  Natively `RUST_LOG=analytics=debug` at run time; the wasm build bakes `RUST_LOG` in at compile
  time (`src/web.rs`).
- `/track` answers `ok <n>`, or `rejected: item <i> rejected: <why> (queued <n>)` (logged as a
  warning) from the engine console.
- In Segment, open the shared source's Debugger and filter on `dcl_renderer_type` (`dao-bevy-web`)
  or on your `session_id`; sends start only once the terms are accepted.

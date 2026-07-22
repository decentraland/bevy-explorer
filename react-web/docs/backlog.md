# react-web — backlog

Design-system, architecture, feature-parity, and bug items for the HUD. Ordered by impact within each
priority. Each item is tagged at the start: `[DS]` design-system primitive / extend / cleanup ·
`[feature]` · `[arch]` · `[bug]`. "Shape" = new primitive / extend existing / pattern.

> Context: we're rebuilding the HUD in `react-web` (replacing the in-engine system-scene/scene-ui).
> This list captures what the old UI had that react-web lacks or reimplements bespoke. Some old
> machinery is deliberately **not** ported (see bottom).

## 🔴 High

1. `[bug]` **HUD hotkeys fire while typing in an SDK7 scene text input** — *behavior/bug, high impact*.
   Typing in a scene-rendered input (search boxes, in-scene forms) triggers the menu shortcuts —
   e.g. pressing **P** opens Settings mid-word. Cause: `useMenuShortcuts` attaches capture-phase
   `keydown` to the **engine iframe window**, and a scene UI input is drawn inside the canvas, so
   `e.target` is the canvas — the `INPUT`/`TEXTAREA`/`isContentEditable` guard never matches.
   Fix path: the HUD needs to know when a scene text input has focus — have the bridge scene relay
   the engine's text-input/IME focus state (a `textInputFocus` message) and suspend `useMenuShortcuts`
   (and any other letter-key hotkeys) while it's true. Same underlying HUD↔engine keyboard-focus
   problem the Enter-to-focus-chat work solved — reuse that mechanism.
2. `[DS]` **Toast system** — *new*. Nothing transient/cross-cutting exists. Needed for real-time events
   (remote friend accepted, community invites, item sold…), ephemeral confirmations, and operational
   errors. Today faked with per-component `setTimeout`. (Old: `notification-toast-stack`.)
3. `[DS]` **`Tabs` primitive** — *new*. Tabs are reimplemented bespoke in ~37 files (Settings,
   Backpack, FriendsPanel, CommunityModal…). (Old: `tab-component.tsx`.)
4. `[DS]` **Reusable `FriendButton` + full relationship model** — *new + pattern*. State is already a
   single reactive source ✅, but the add-friend CTA is duplicated per view (ProfileCard,
   ProfilePassport, CommunityModal) with ad-hoc optimism. Need `<FriendButton address>` /
   `useRelationship`, a **6-state** relationship (add `blocked`, `incoming`/Accept to the current 3),
   centralized optimistic update, and **fix CommunityModal desync** (it reads `member.isFriend`, not
   `session.friends`).
5. `[DS]` **`Button`: `loading` state + `danger`/`destructive` + `link`/text variant** — *extend*.
   Recurring need (jump-in/create/send → loading; unfriend/reject/leave/delete → danger; a subtle
   underlined text-link like the gate's "try anyway…" → link, currently a bespoke `<button>`). (Old:
   `ButtonComponent`.)
6. `[bug]` **"Jump in" icon on the initial scene catalog is actually a pencil** — *visible on first
   impression*. `JumpInGlyph()`'s SVG path (`M5 12l9-9 4 4-9 9-5 1z` + `M13 4l3 3M5 12l-1 7 7-1`) draws a
   diagonal pencil-with-tip shape, not a "jump in" arrow — a copy/paste-wrong-glyph mistake, not a design
   choice. **Duplicated identically** in both `PlaceCard.tsx` and `FeaturedCard.tsx` (the login-flow
   "Live Now"/"Featured Places" catalog, `LoadingAndLogin.tsx`), so fix both — or better, extract one
   shared glyph while fixing it (there's no `src/design/` icon for this yet).

## 🟡 Medium

7. `[feature]` **Engine-panic / error capture → popup** — *new* (largely SHIPPED — `ErrorBoundary` +
   `EngineErrorModal` + crash watchdog landed on `fix/react-web-hud`). Kept as a tracking entry: engine
   WASM panics on launch (e.g. `can't init wasm queue`) and runtime crashes now surface a popup
   (message + copy details + reload/dismiss). (Old: `error-popup` + `error-popup-service`.)
8. `[DS]` **`showDialog` / `showConfirm` (imperative dialog helpers)** — *mostly DONE (PR #915)*. Added
   `openPopup` + `showDialog` + `showConfirm` + `<PopupHost/>` in `src/design/popups.tsx` — an
   imperative, stackable popup layer backed by a **module-level store** (like the `hoverPos` store,
   read by the single `<PopupHost/>` via `useSyncExternalStore`), callable from anywhere without a
   hook or prop-threading (a popup can open a popup). `showDialog({ title, body, actions })` renders a
   ModalShell with an arbitrary footer (confirm / alert / custom multi-action) and resolves the chosen
   action id (Promise); `showConfirm` is sugar resolving a boolean. The profile-card Block confirm
   runs on it (`if (await showConfirm(…)) …`). **Remaining:** migrate the still-bespoke confirms
   (`WorldVisitModal`, `ExitConfirm`) + the passport/community chains onto `openPopup`.
   (Old: `confirm-popup` / `alert-popup`.)
   Extra reason to finish the migration: `hasOpenPopup()` (the store predicate that suppresses the
   HUD's Enter/"Chat" action while an overlay is up — see `requestFocusChat` in `useEngineSession`)
   only sees the popup stack. `WorldVisitModal` and `ExitConfirm` are conditionally rendered from
   App-local state (`visitWorld`, `exitGuard.confirming`), so Enter still opens + focuses the chat
   behind them. Moving them onto `openPopup` fixes that for free; the alternative (wiring their state
   into the hook) is the reason not to bother.
9. `[bug]` **`closeTopPopup` / `closeById` never run the popup's `onClose` → `showDialog` Promise leak** —
   *correctness footgun, P2 pending PR #915 review*. `closeTopPopup()` (fired by the engine
   `Cancel`/Escape relay in `useEngineSession`) removes the top node with `closeById`, which only
   does `stack.filter + emit` — it never invokes that node's dismiss contract. So a
   `showDialog`/`showConfirm` closed through the engine path never resolves its Promise (the `await`
   hangs; the closure leaks). Harmless today for the only consumer (the profile-card Block confirm):
   `ModalShell`→`Modal` traps focus and has its own DOM `Escape`→`onClose`, which resolves `false`
   first, so the later engine-`Cancel` `closeById` is an idempotent no-op. It bites the moment a
   future `showDialog` is dismissed via the engine path while the engine holds keyboard focus (no DOM
   keydown reaches `Modal`). Fix: settle on removal — store the `close`/`onDismiss` per stack node and
   have `closeById` call it, so every close path (engine `Cancel`, backdrop click, per-node `close`)
   resolves the Promise exactly once.
10. `[DS]` **`Badge` (standalone)** — *extract*. Badge logic is trapped inside `IconButton`; can't put a
    badge on a tab/avatar/chip without reimplementing. (Old: `notification-badge.tsx`.)
11. `[DS]` **`Chip` / `Tag`** — *new*. "chip" is bespoke in ~11 files (map categories, count pills,
    status). (Old: `color-tag.tsx`.)
12. `[DS]` **Consolidate modals onto `Modal`/`ModalShell`** — *cleanup*. ProfileCard, CommunityModal,
    CommunityCreateModal, WorldVisitModal roll their own portal/overlay and hardcode `z-index: 10001`.
    Unify backdrop / escape / focus-trap / z-layer.
    12b. `[bug]` **Suppress the world-hover tooltip while any overlay/scrim is open** — *mechanism, from
    PR #915 review*. A scrim freezes the engine raycast, so no hover-exit fires; the world-hover prompt
    (`<Pointer>`) can stay painted behind/beside a popup. Today only the `avatarClick` path clears it
    (a per-message `setHover([])` in `useEngineSession`), which doesn't scale — the next world-entity
    click that opens a popup needs its own clear. The deciding factor is trigger origin, not the
    overlay: DOM-triggered popups (chat/friends/menus) are safe because reaching them crosses free
    canvas and fires the exit; only world-entity clicks drop the scrim onto the hovered entity with no
    exit. Clean fix, once the scrims are unified here: (a) the shared scrim/Modal primitive publishes an
    "overlay open" signal (context or ref-count); (b) `<Pointer>` gates its hover hints on that signal
    (render-level suppression, **don't** mutate `session.hover` — that just relocates the special case
    and, because the frozen `hoverPos` is stale, can flash a mispositioned prompt on close); (c) drop
    the per-message `setHover([])`. Covers every popup, present/future, for free. *(Point 1 of the
    review — tooltip only returns after a 1px move on close — is expected native tooltip behavior and is
    not addressed by this; leave as-is.)*
13. `[DS]` **`Radio` / `RadioGroup`** — *new*. Have Checkbox/Toggle/Select but no Radio; bespoke in
    PermissionDialog. (Old: `radio-button.tsx`.)
14. `[DS]` **`Skeleton`** — *new*. Only `Spinner` exists; no load placeholders for lists/cards.
    (Old: `loading-placeholder.tsx`.)
15. `[bug]` **Passport "too far by camera" is confusing in third-person** — *behavior/UX, engine+bridge*.
    Hovering a nearby avatar in third-person greys out "Show Profile" with "Get camera closer" even
    when your avatar is standing right next to them. Two causes combine: (1) the avatar pointer in
    `bridge-scene/src/domains/avatarPointer.ts` sets **no** distance fields, so it hits the SDK7
    default (camera distance ≤ 10m — `passes_distance_check`, case 4 in
    `crates/scene_runner/src/update_scene/pointer_results.rs`); (2) bevy's `camera_distance` is the
    **raw camera-origin→hit** distance, so a pulled-back third-person boom inflates it past 10m. The
    OR machinery you'd reach for already exists (`max_distance` OR `max_player_distance`), so adding
    an OR to the default alone won't fix it — the camera leg is still boom-inflated. **How
    unity-explorer avoids it** (verified in `../unity-explorer`): its passport interaction
    (`ProcessOtherAvatarsInteractionSystem.cs`) has **no** camera-distance gate at all — just a 100m
    ray + privacy modifiers (`GlobalInteractionPlugin.cs` injects `maxRaycastDistance = 100f`); and
    even for scene entities its `MaxDistance` ("camera") check is measured from the **player focus**
    in third-person, never the camera boom (`PlayerOriginatedRaycastSystem.cs:85` —
    `FirstPerson ? hitInfo.distance : Vector3.Distance(hit, PlayerFocus.position)`). So Unity's range
    is player-relative everywhere and the boom is invisible to gating. **Fix paths:** (a) *cheap,
    bridge-only* — set `maxPlayerDistance` on the avatar "Show Profile" pointer so it's gated on
    player proximity (camera-agnostic, Unity passport parity); the existing `tooFarReason` plumbing
    then only ever reports `player`. (b) *faithful, engine + ~15-20min wasm rebuild* — make bevy's
    interaction `camera_distance` player-focus-relative in third-person like Unity, fixing **all**
    SDK pointer events, not just passports. Analysis-only for now (2026-07-09) — user deferred the
    code change.
16. `[feature]` **Passport — finish the sections (feature parity with unity-explorer / bevy-ui-scene)**
    — *feature parity*. The passport has OVERVIEW/BADGES/PHOTOS tabs but is missing sections the old
    scene renders (`bevy-ui-scene`: `ui-classes/main-hud/passport/passport-popup.tsx`). Gaps that
    **need new bridge/protocol data:** (a) **Equipped Wearables + Emotes** of the *viewed* avatar (grid:
    rarity-tinted card + name + rarity tag + click → marketplace) — today `Wearable` only exists for
    your OWN backpack, not another user's passport. (b) **Richer badges** — add `category`
    (Explorer/Collector/Creator/Socializer/Builder), `completedAt`, and in-progress `progress
    {current,total}` to `Badge` (today just `{id,name,tier,image}`). (c) **Profile fields** `country` +
    `sexualOrientation` in `ProfileInfo` (has the other 9). **UI-only (data mostly present):** (d)
    Badges tab category-filter row + per-badge date / progress bar (once (b) lands). (e) Passport-header
    **⋮ menu** (Block/Unblock · Report · Invite to Community) — reuse the world `ProfileCard`'s action
    set. (f) Wire the **3D avatar preview** into the passport (machinery exists —
    `setEngineViewport('avatarPreview')`, used by the Backpack) instead of the 2D snapshot, + 3D badge
    preview on the Badges tab.
17. `[feature]` **Notifications panel — bounded height, load-on-scroll, click-through to a detail
    popup** — *feature/bug*. `NotificationsPanel.tsx` fixes `.root` to `top: 16px; bottom: 16px`
    (`NotificationsPanel.module.css`), so the panel is always full-viewport-tall regardless of content —
    both `bevy-ui-scene` (`notifications-menu.tsx`, a fixed `menuHeight = fontSize * 30 * 1.1`) and
    unity-explorer size it to a bounded, content-driven height instead. Most of the time only the most
    recent 1–2 notifications matter, so a full-height panel is wrong by default; bound the height
    (roughly N rows tall, scrollable within that) closer to the old panels. Two follow-on gaps found
    alongside this: (a) **load-on-scroll / pagination** — the bridge fetches the whole notification list
    in one shot (`fetchNotifications`); `bevy-ui-scene`'s own fetch util already accepts `from`/`limit`
    (`notifications-promise-utils.ts`, default `limit: 50`) so the backend supports paging even though
    the old scene doesn't use it either — unity-explorer's list (`NotificationsPanelController`, built on
    a `SuperScrollView` `LoopList`) does load incrementally. react-web would need to thread `from`/`limit`
    through the `getNotifications` bridge request. (b) **clickable rows → a detail popup** — rows are
    inert today (no `onClick`); tapping a notification should open more detail (and, for actionable types
    like community invites, act on it). **Open sequencing question**: is a one-off popup component worth
    building now, or should this wait on **stackable popups** — i.e. #12 (consolidate modals onto
    `Modal`/`ModalShell`) landing first, so a notification-detail popup doesn't become yet another bespoke
    overlay to migrate later. Leaning toward doing #12 first if both are picked up.
18. `[arch]` **Pointer-lock / camera-look coordination across the HUD — the "right-click to move the
    camera again" confusion** — *behavior/parity, review + decision*. On web the engine's camera-look
    *is* the browser Pointer Lock (bevy `CursorGrabMode::Locked` → `requestPointerLock` on the iframe
    canvas; see `crates/user_input/src/camera.rs` `update_cursor_lock` +
    `crates/scene_runner/src/update_scene/pointer_lock.rs`). Because the React chat/menus are DOM
    surfaces **outside** the iframe, opening one has to free the cursor by releasing the engine lock
    (`PointerLock.isPointerLocked = false` on `CameraEntity` — done today only for the profile-card
    `avatarPointer.ts` and chat `chat.ts`). The catch: **the browser can't re-lock without a fresh
    user gesture**, and the React→bridge→engine hop loses that gesture, so **camera-look does NOT
    auto-resume** — after closing chat/a menu the player must **right-click (or click) into the world**
    to re-engage the camera, which is not discoverable and confuses users. Two things to do here: **(a)
    the review/decision:** evaluate **mirroring an in-engine chat input** — route chat keystrokes through
    the engine's own `TextEntry` (`crates/ui_core/src/text_entry.rs`, which just re-prioritises the
    keyboard via `reserve(InputType::Keyboard, InputPriority::TextEntry)` and **never drops the pointer
    lock**) and relay its value to the React chat display over the bridge. That restores the old
    `bevy-ui-scene` seamless feel (type → close → camera resumes instantly, no click) at the cost of
    re-introducing engine-owned input + a split display/input source of truth — hence a decision, not a
    given. (Escape is a browser limit today, but there IS a supported escape hatch we're deferring:
    JS-initiated **fullscreen + the Keyboard Lock API** — `navigator.keyboard.lock(['Escape'])` then
    `requestFullscreen()` — delivers Escape to the page as a normal keydown and keeps pointer lock, with
    a hold-Esc-2s exit the browser narrates; this is the pattern cloud-gaming clients (Stadia, GeForce
    NOW) use. Chromium-only, which is fine — we only support Chromium (see the no-GPU gate) — but it
    needs whole-session fullscreen, must exclude the `?hud=0` embed, and would double-deliver Escape
    (DOM + the engine's `Cancel` action) so one source has to win. A follow-up on its own, deferred to
    spend time on higher-priority issues; the fragile `exitPointerLock()`-then-gesture-less-
    `requestPointerLock()` trick is NOT it — don't rely on it. Moot on native: no browser pointer lock
    there, the engine owns the OS cursor grab and releases it on the `Chat` action.) **(b) the related bug — web part SHIPPED (`fix/03-backpack`):**
    full-screen menus (settings/map/backpack/communities/places/gallery) opened via their **hotkey while
    camera-look is active** used to render over a still-locked cursor (only profile-card + chat freed it).
    Now a single rule in `useEngineSession` releases the lock whenever any full-screen menu opens (a
    `useEffect` on `menuPageOpen` → `document.exitPointerLock()`), replacing the per-trigger frees on web.
    **Remaining — native:** `exitPointerLock` is a web no-op, so on native (CEF, engine owns the OS cursor
    grab) a menu open does **not** yet free the cursor; if that proves needed, relay a bridge write
    `PointerLock.isPointerLocked = false` on `CameraEntity` on menu open, the way `chat.ts` /
    `avatarPointer.ts` already do for their surfaces. Kept Medium: chat + click-to-open panels + web
    menus work today; only native menu-open cursor release is open.
19. `[feature]` **Chat links: confirm popup before teleporting and before opening a URL** — *parity with
    `bevy-ui-scene`, safety*. The **parsing** has parity: `chatText.tsx`'s `TOKEN_RE` linkifies the same
    four kinds as the old `LINK_TYPE` (`components/chat/chat-message/ChatMessage.tsx:43`) — url, world,
    location, mention — and react's single-pass regex is the better design (the old one composed four
    sequential string replacers, so a URL containing `…?a=1,2` got a `<link=location::1,2>` injected
    inside it). The gap is in **what a click does**: the old HUD gated *both* teleports and URLs behind
    a confirm; react-web only gates worlds.
    - **Coordinates** (`10,-5`) — react teleports on click, no confirm: `onLocation` → `Chat.onTeleport`
      → `session.map.teleport` (`App.tsx:235`). Old: `ui-classes/main-hud/popup-teleport.tsx:142` —
      "Are you sure you want to be teleported to **x,y**?" plus the fetched scene title + thumbnail, and
      "This will also change your realm to **realm**" when the target realm differs; CONTINUE →
      `changeRealm` then `teleportTo`.
    - **URLs** — react renders a bare `<a target="_blank" rel="noreferrer noopener">`, so any stranger's
      link in Nearby opens in one click. Old: `ui-classes/main-hud/popup-url.tsx:59` — "Are you sure you
      want to follow this link? Continuing will open the link in the browser. Make sure it's a website
      you trust before proceeding." with the URL shown; CONTINUE → `openExternalUrl`.
    - **Worlds** (`boedo.dcl.eth`) — already confirmed via `WorldVisitModal` ✅.
    Build the two missing ones on `openPopup`/`showConfirm` rather than new App-local modals, and fold in
    `WorldVisitModal` while there (see item 8 — it also fixes the Enter-focus hole). Related, same pass:
    (a) react linkifies `http://` too, where the old regex was `https://`-only — neither validates the
    scheme, the old one just incidentally kept `http`/`javascript:`/`data:` out of the click path, so
    make the scheme check explicit; (b) `PlacesPage` (`App.tsx:263`) changes realm with **no** confirm,
    so the same `onVisitWorld` prop name means "confirm first" in Chat and "just go" in Places — pick
    one; (c) `chat.rich.test.tsx` covers url/location/mention clicks but never the `world` token or the
    `rel` attribute.

20. `[arch]` **Unify avatar-equip authority — bridge re-emits `wearables` after every mutation (+
    mutation acks/errors)** — *hardening, from the fix/ui/04-outfits architecture review*. The equipped
    set runs two consistency models today: `equipOutfit` is **bridge-authoritative** (resolves the
    outfit by urn via `resolveEquippedSet` and re-emits `wearables` — the fix/ui/04-outfits fix), but
    single-item `equip` is **client-authoritative** — `applyEquippedOptimistic` (`useEngineSession.ts`)
    rebuilds `equippedWearables` locally and the bridge never re-emits, so the optimistic state is
    load-bearing, not a hint. Consequences: (a) **a failed `setAvatar` deploy diverges HUD from avatar
    permanently** — the failure dies in a bridge `console.error` (`wearables.ts` `'equip'`), the HUD
    keeps the item marked equipped, and the *next* equip (built via `equipSetWith` from that phantom
    set) deploys the divergence into the profile; (b) **mock/real contract asymmetry** — the mock
    re-emits `wearables` after `equip` (`mockBridge.ts`) but the real bridge doesn't — the same masking
    asymmetry that hid the outfits off-page P1 (`?mock=1` validates a contract production doesn't
    honor). Fix path: every avatar-mutation handler ends by re-emitting the authoritative set via
    `resolveEquippedSet` — on success *and* on failure (failure re-emits the previous set = visual
    rollback) — demoting the client optimistic update to instant feedback the emit confirms or
    corrects; epoch/sequence the `wearables` emits (the `requestId` pattern `catalogQuery`/`catalogPage`
    already use) so a stale emit can't clobber a newer optimistic set — this also closes the
    equip-during-`equipOutfit`-round-trip race. Same pass: mutation ack/error correlation so the HUD
    can surface "equip failed" (silent today). Fold in the state normalization this implies: derive
    the grid card markers from the equipped set at render and retire the per-item `equipped` flag on
    catalog page items (+ the page-priority arbitration in `equippedByCat`) — the same fact currently
    lives in two states, and any mutation path that updates only one shows stale markers/slots (bit
    once already: equipOutfit's authoritative emit didn't reconcile page flags — fixed by
    reconciliation in the session's `wearables` handler, but normalization makes the class
    impossible). Related hardening: the urn logic all this leans on
    (`itemUrnOf`, `tokenUrnFor`, `resolveEquippedSet`) has **zero coverage** — bridge-scene has no test
    runner and the app's vitest excludes it; they're pure functions, so add a minimal bridge-scene
    vitest (or move them into a shared, tested module).

## 🟢 Low / when a feature needs it

21. `[DS]` **`Divider`** (bespoke in ~4 places · old `bottom-border`)
22. `[DS]` **`Pagination`** (unused today · old `pagination/`)
23. `[DS]` **`CopyButton`** (inline in ProfileCard · old `copy-button`)
24. `[DS]` **`Username`** (name + verified · old `player-name-component`)
25. `[DS]` `Button` `iconLeft`/`iconRight` props + `hoverIcon` (niche · old `ButtonComponent`)
26. `[feature]` **Re-enable "Invite to Community" in `ProfileCard`** — *feature, parked until
    communities work*. The row/submenu UI was removed from `ProfileCard` (PR #915 follow-up); the
    protocol messages, `session.communities.invitable`/`requestInvitable`/`invite`, and the bridge
    handlers all remain. When re-enabling: (1) the `/invites` response is `{data:[…]}` but `signed()`
    already unwraps the envelope — type it as the bare array (fixed in `communities.ts`, don't regress
    it); (2) the `invitableFetchedRef` once-per-address cache needs invalidation — drop the key on
    fetch failure (a transient 500 currently caches "no communities" for the session), remove/refetch
    after a successful invite (else the card re-offers it and the duplicate POST fails silently), and
    clear both `invitable` and the ref on logout/identity change; (3) surface invite errors to the user
    (the bridge currently swallows them with `console.error`); (4) build the submenu on the
    `ContextMenu` primitive instead of the removed bespoke `.submenu`/`.subRow` CSS.
27. `[arch]` **HUD state: `useEngineSession` hook prop-drilled → consider Context / a store** —
    *architecture, low priority*. All HUD state lives in one `useEngineSession` hook at the top of
    `Hud`, prop-drilled down; the returned `session` is a fresh object every render, so the whole HUD
    re-renders on any change. Fine at current scale (engine round-trips are the bottleneck, not React
    renders), so **not urgent**. Nuance if we ever move it: **Context alone doesn't fix re-renders** — a
    single `SessionContext` only removes prop-drilling (ergonomics), because the value changes every
    render. Targeted re-renders (only friends consumers re-render on a friends change) need **memoized
    slices** (the `friends`/`chat` objects are plain literals today) **plus** either split per-domain
    contexts or a **selector store** (zustand/jotai — adds a dep; project is deliberately
    state-lib-free). Also a test cost (harness passes props today; Context needs a provider wrapper).
    Recommendation: keep prop-drilling; add a single `SessionContext` only if drilling ergonomics annoy;
    memoized slices / store only if re-renders become a *measured* problem. **One concrete exception to
    "renders aren't the bottleneck": item 37 — `proximity` is a per-frame (~60/s) re-render source while
    near an interactable, not a user-action change.**
28. `[arch]` **Deep-linkable / bookmarkable navigation — reflect location in the URL** — *architecture,
    low priority*. Entering a scene/world (and, ideally, opening HUD surfaces like the map/backpack)
    should be **parameterized in the URL** so the state is shareable and bookmarkable: reload/paste a
    URL and land in the same realm + coords. Scope to nail down: realm/world + parcel coords (e.g.
    `?realm=…&position=x,y` or a path), whether HUD panels also serialize, and wiring it to the picker
    (`pickDestination`) + `map.teleport`/`changeRealm` so URL ⇄ engine stay in sync (`popstate` → jump,
    jump → `pushState`). Deferred: needs a small router/URL-sync layer (project is router-free today).
29. `[arch]` **Migrate inline `dt`-throttle timers to `bridge-scene/src/system-helpers.ts`** —
    *cleanup*. The `throttleByDt` helper (added in PR #915 for `avatarPointer`) replaces a dt-accumulator
    that's re-implemented inline across most bridge domains (`chat`, `world`, `friends`, `project`,
    `nametags` — two timers there —, `avatarPreview`). Migrate them to the helper (and consider
    `singleFlight` / a named `pollSequential` wrapper where a polled async RPC could overlap). Pure
    cleanup, no behavior change; kept out of #915 to stay surgical.
30. `[feature]` **Voice feedback — "who's speaking" indicator** — *feature parity, when voice chat is
    prioritized*. The old scene showed an **animated speaking indicator on each avatar nametag** while a
    nearby player talked (and used the local mic state for your own tag). Mechanism to port: the engine
    exposes a voice stream — `BevyApi.getVoiceStream()` yielding `{ sender_address, active }`
    (`MicActivation`); the old scene folded it into a `playerVoiceStateMap[address] = active` and
    animated a `voice-N` sprite on the tag (`bevy-ui-scene`:
    `components/avatar-tags/avatar-name-tag-3d.tsx` + `tag-element.tsx`). For react-web: a **new bridge
    domain** subscribing to that stream and forwarding `{ address, active }` over the channel; session
    state (a `speaking` set keyed by address); and the indicator surfaced on the **engine-rendered
    nametags** (bridge scene) and/or the DOM **nearby-members list** (`chat.members`) — e.g. a pulsing
    mic ring. Depends on voice chat being wired end-to-end; today only the local `mic` toggle exists (no
    per-remote-speaker signal). Could yield a reusable `SpeakingIndicator` primitive.
31. `[feature]` **Re-add "Report" to the profile card once a moderation/report endpoint exists** —
    *feature, low priority*. Report was removed from `ProfileCard` in PR #915 because there's no backend —
    it was only a `console.log` stub, so shipping a dead action was worse than hiding it. When a report/
    moderation endpoint lands: re-add the `Report` row + `onBlock`-style `onReport` request prop
    (parent-owned confirm, same pattern as Block), the `ReportIcon` glyph, and wire the actual submit.
    (Old scene logged too — this is genuinely new backend work, not just UI.)
32. `[feature]` **Passport / own-profile edit mode — no UI yet** — *feature, own-profile only; flagged by
    Rob*. bevy-ui-scene lets you edit your own passport in place — About Me, the info-field dropdowns,
    links (add/remove, up to 5), and display name — then deploys the updated profile. react-web can
    *view* the profile (`ProfilePanel` = own profile, `ProfilePassport` = others) but has **no UI to
    edit your own** display name, description/bio, links, etc. — completely unimplemented (no
    `editProfile`/`deployProfile` path in `features/profile` or the session). Needs the edit surface +
    wiring the profile deploy through the bridge/engine. Larger than the view-parity item (#16) — hence
    separate and lower priority than showing OTHER users' passports correctly. Reference the old client
    for the flow (`unity-explorer` `Explorer/Assets/DCL/UI/`, `bevy-ui-scene` profile screens).
33. `[feature]` **Chat rate limiting** — *hardening, not in bevy-ui-scene*. unity-explorer's
    `MultiplayerChatMessagesBus` dedupes + rate-limits + buffers sends; react-web (like bevy-ui-scene)
    sends on every Enter with no client-side throttle. Only worth adding if spam becomes a real problem
    server-side rate limiting doesn't already cover.
34. `[feature]` **DMs / private chat channels** — *net-new, not a port*. Neither `bevy-ui-scene` nor
    today's react-web have anything beyond the single "Nearby" channel; unity-explorer's
    `ChatChannelsPresenter`/`ChatChannelType.USER` is the only prior-art reference. Large scope (channel
    list UI, per-conversation history, member-list → "message" entry point) — flag for a dedicated design
    pass, not a drive-by addition.
35. `[arch]` **SSO redirect on the login screen throws away the in-flight engine WASM download** —
    *boot perf, not blocking / not a direct bug*. On the login/loading screen the engine WASM is
    already downloading in the background while the sign-in buttons are live. "Start with account" /
    "Use different account" call `redirectToAuth()` → `location.replace('/auth/login?redirectTo=…')`
    (`src/features/auth/sso.ts`), a **same-tab, same-page navigation** that tears down the document and
    **cancels the partial WASM download**; when the auth site bounces back, the download restarts from
    zero, so the user waits through it twice. Doesn't block anyone and isn't wrong per se — just wasted
    bytes + a slower perceived boot on the primary flow. Options to evaluate: (a) run the auth handoff
    in a **new tab** (`window.open(authLoginUrl(), '_blank', 'noopener')` — mind the noopener/reverse-
    tabnabbing rule) and detect the returned identity in the original tab (poll the SSO localStorage
    keys / `storage` event) so this document — and its download — survives; or (b) run the auth flow in
    a hidden **iframe** and read back the identity via `postMessage`/`storage` (auth site must allow
    being framed same-origin — verify its CSP/`X-Frame-Options`). Both are more moving parts than the
    current straight redirect, so only worth it if boot time on login is a measured concern.
36. `[feature]` **Radial hover prompts — viewport clamping near screen edges** — *polish, non-blocking;
    point to review*. When the free cursor is near a viewport edge, the fixed-offset radial slots
    (`HOVER_SLOTS` in `features/pointer/Pointer.tsx`) that point toward that edge run off-screen (cursor
    at the right edge → the right-middle prompt's label clips). No clamp today. Arguments for leaving it:
    (a) it's a **cursor-anchored** prompt, not a static web tooltip, so at the extreme edge some
    overflow is inherent (the cursor is already at the edge, and the OS cursor itself can sit partly
    off-screen there); (b) the canonical **unity-explorer** does the same — `ShowHoverFeedbackSystem`
    + `HoverCanvas` position a fixed layout (`CURSOR_LAYOUTS`) at the cursor with **no edge-clamp/flip**
    either (only `text-overflow: ellipsis` on the label), so a clamp would be an *enhancement over the
    reference*, not a parity gap. If addressed: clamp the container by its measured (scaled) bounds like
    `ProfileCard`, or flip slot sides near the edge. Review comment (note: it says "root is
    overflow: hidden" — there's no such rule, the clip is just the viewport edge):
    https://github.com/decentraland/bevy-explorer/pull/915#discussion_r3529180273
37. `[arch]` **`proximity` pushes a full HUD re-render every frame while near an interactable** — *perf,
    when profiling confirms*. Unlike most session changes (user actions), the proximity domain is a
    **per-frame** source: `registerProximity`'s `ctx.push` reprojects each in-range entity world→screen
    and calls `ctx.send({ kind: 'proximity', tips })` **every frame with no dedupe**
    (`bridge-scene/src/domains/proximity.ts:52`) whenever ≥1 interactable is in range. Each message
    hits `setProximity(msg.tips)` with a fresh array (`useEngineSession.ts:416`), so `App` — and, per
    item 27, the whole tree — re-renders ~60×/s. This is the concrete counterexample to item 27's
    "engine round-trips are the bottleneck, not React renders." Cost is **conditional**: zero when
    nothing is in range (the `inRange.size === 0` early-out short-circuits the send), but scales with
    tree size, per-render cost (e.g. the `notifications.reduce` unread count runs every render), in-range
    entity count (busy scene = bigger `tips` + more projection work), and device CPU; worst case it
    steals main-thread from the same-origin engine iframe → world frame drops + battery drain near
    interactables. **Measure first** (React Profiler: proximity commit duration × 60) — <1ms is noise,
    5–10ms is a real 30–60% main-thread tax. Fix (pattern already in the repo): mirror the `hoverPos`
    module store (item 8) — `<Pointer>` is the sole consumer, so move proximity off session state into a
    store it reads via `useSyncExternalStore` (as it already does for `hoverPos`); then 60/s updates
    re-render only `<Pointer>`, not the tree. Optional bridge-side dedupe (skip `ctx.send` when `tips`
    is unchanged) zeroes the standing-still case but not the moving one (positions legitimately change
    each frame), so the store is the structural fix.
38. `[bug]` **Chat name click shows the raw address for players who left nearby range** — *UX regression,
    P2 pending PR #915 review*. `Chat`/`FriendsPanel` now open the shared card via
    `openProfileCard(user.address, …)` (address only); the container re-resolves name/picture with
    `resolveIdentity` (nearby roster → friends/requests → fetched passports). For a **non-friend who
    has since left `chat.members`**, nothing resolves, so the card shows the bare `0x…` address instead
    of the display name that was in the historical message (the old `ChatUser`-carrying path preserved
    it). Common cases (nearby / friends) are unaffected. Fix if it matters: pass the message's known
    name/picture into `openProfileCard` as a fallback hint, or give `resolveIdentity` a small
    last-seen name cache.

## Not gaps (already good / ahead)

`Modal` (portal + focus-trap + blur + `--ui-scale`, richer than the old backdrop), `IconButton`
(badge + tooltip + shortcut), the **friend-state architecture** (single reactive source, simpler than
the old version-bump), `tokens.css`, and primitives the old lacks (`WearableCard`, `EmptyState`,
`PageHeader`, `CharCounter`, `SearchField`, `ContextMenu`).

## Deliberately NOT ported

- The old Redux `shownPopups` popup-stack **store/type-registry** — React composition + portals handle
  the *stacking* (z-order) for free, so the Redux store + `HUD_POPUP_TYPE` enum weren't ported. The
  *imperative-open pattern* it enabled (open a popup from anywhere, popups open popups) **was** kept —
  as `openPopup`/`showDialog` + `<PopupHost/>` — a module store rendering JSX directly (no type map,
  no Redux store/dispatcher). `<PopupHost/>` is a top-level layer; the popups it renders use
  `ModalShell`, which is what portals to `document.body` (to escape the HUD `--ui-scale` transform).
  See item 8.
- The `friendshipStateVersion` + cached-snapshot + event-bus machinery — an artifact of the SDK7
  per-frame render model; React's targeted re-renders make it unnecessary.

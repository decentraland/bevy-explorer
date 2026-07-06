# react-web — design-system & architecture backlog

Gaps found by auditing the old system-scene (`~/dev/protocol-squad/bevy-ui-scene`) against
`react-web/src/design`. Ordered by impact. "Shape" = new primitive / extend existing / pattern.

> Context: we're rebuilding the HUD in `react-web` (replacing the in-engine system-scene/scene-ui).
> This list captures what the old UI had that react-web lacks or reimplements bespoke. Some old
> machinery is deliberately **not** ported (see bottom).

## 🔴 High

1. **Enter should focus the chat input (core in-world interaction, currently broken)** — *behavior/
   bug, high impact*. Pressing **Enter** in-world doesn't focus the chat input, so you can't start
   typing the standard way (DCL/game convention: Enter focuses chat → type → Enter sends → Escape
   blurs). The crux: while the engine iframe holds keyboard focus (pointer-locked camera-look) the
   keypress reaches the **engine iframe**, not react-web's DOM, so a plain `keydown` listener never
   sees it. Fix path: capture it globally like `useGlobalHotkey` (already survives engine-iframe focus
   — the FPS toggle uses it) to focus the chat input, **or** have the engine/bridge emit a "focus
   chat" event (the old scene owned chat focus in-engine). Also free the pointer + stop keystrokes
   reaching the engine while typing (so the avatar doesn't move), and restore on blur/Escape. (Old:
   bevy-ui-scene chat Enter-to-focus.)
1b. **HUD hotkeys fire while typing in an SDK7 scene text input** — *behavior/bug, high impact*.
   Typing in a scene-rendered input (search boxes, in-scene forms) triggers the menu shortcuts —
   e.g. pressing **P** opens Settings mid-word. Cause: `useMenuShortcuts` attaches capture-phase
   `keydown` to the **engine iframe window**, and a scene UI input is drawn inside the canvas, so
   `e.target` is the canvas — the `INPUT`/`TEXTAREA`/`isContentEditable` guard never matches.
   Fix path: the HUD needs to know when a scene text input has focus — have the bridge scene relay
   the engine's text-input/IME focus state (a `textInputFocus` message) and suspend `useMenuShortcuts`
   (and any other letter-key hotkeys) while it's true. Same underlying HUD↔engine keyboard-focus
   problem as item 1 — solve them together.
2. **Toast system** — *new*. Nothing transient/cross-cutting exists. Needed for real-time events
   (remote friend accepted, community invites, item sold…), ephemeral confirmations, and operational
   errors. Today faked with per-component `setTimeout`. (Old: `notification-toast-stack`.)
3. **`Tabs` primitive** — *new*. Tabs are reimplemented bespoke in ~37 files (Settings, Backpack,
   FriendsPanel, CommunityModal…). (Old: `tab-component.tsx`.)
4. **Reusable `FriendButton` + full relationship model** — *new + pattern*. State is already a single
   reactive source ✅, but the add-friend CTA is duplicated per view (ProfileCard, ProfilePassport,
   CommunityModal) with ad-hoc optimism. Need `<FriendButton address>` / `useRelationship`, a
   **6-state** relationship (add `blocked`, `incoming`/Accept to the current 3), centralized optimistic
   update, and **fix CommunityModal desync** (it reads `member.isFriend`, not `session.friends`).
5. **`Button`: `loading` state + `danger`/`destructive` + `link`/text variant** — *extend*. Recurring
   need (jump-in/create/send → loading; unfriend/reject/leave/delete → danger; a subtle underlined
   text-link like the gate's "try anyway…" → link, currently a bespoke `<button>`). (Old:
   `ButtonComponent`.)

## 🟡 Medium

6. **Engine-panic / error capture → popup** — *new* (ACTIVE — see plan). No `ErrorBoundary`, no
   `unhandledrejection`/`window.onerror` handler, no global error surface. The engine WASM can panic
   on launch (e.g. `can't init wasm queue`) and today it only hits the console. Capture engine panics
   + uncaught errors and show the user a popup (message + copy details + reload). (Old: `error-popup`
   + `error-popup-service.showErrorPopup`.)
7. **`useConfirm` / `showAlert` (imperative dialog helpers)** — *new*. `Modal`/`ModalShell` exist but
   each confirm is rebuilt (WorldVisitModal, ExitConfirm). (Old: `confirm-popup` / `alert-popup`.)
8. **`Badge` (standalone)** — *extract*. Badge logic is trapped inside `IconButton`; can't put a badge
   on a tab/avatar/chip without reimplementing. (Old: `notification-badge.tsx`.)
9. **`Chip` / `Tag`** — *new*. "chip" is bespoke in ~11 files (map categories, count pills, status).
   (Old: `color-tag.tsx`.)
10. **Consolidate modals onto `Modal`/`ModalShell`** — *cleanup*. ProfileCard, CommunityModal,
    CommunityCreateModal, WorldVisitModal roll their own portal/overlay and hardcode `z-index: 10001`.
    Unify backdrop / escape / focus-trap / z-layer.
    10b. **Suppress the world-hover tooltip while any overlay/scrim is open** — *mechanism, from PR #915
    review*. A scrim freezes the engine raycast, so no hover-exit fires; the world-hover prompt
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
11. **`Radio` / `RadioGroup`** — *new*. Have Checkbox/Toggle/Select but no Radio; bespoke in
    PermissionDialog. (Old: `radio-button.tsx`.)
12. **`Skeleton`** — *new*. Only `Spinner` exists; no load placeholders for lists/cards.
    (Old: `loading-placeholder.tsx`.)

## 🟢 Low / when a feature needs it

13. **`Divider`** (bespoke in ~4 places · old `bottom-border`)
14. **`Pagination`** (unused today · old `pagination/`)
15. **`CopyButton`** (inline in ProfileCard · old `copy-button`)
16. **`Username`** (name + verified · old `player-name-component`)
17. `Button` `iconLeft`/`iconRight` props + `hoverIcon` (niche · old `ButtonComponent`)
17b. **Re-enable "Invite to Community" in `ProfileCard`** — *feature, parked until communities work*.
    The row/submenu UI was removed from `ProfileCard` (PR #915 follow-up); the protocol messages,
    `session.communities.invitable`/`requestInvitable`/`invite`, and the bridge handlers all remain.
    When re-enabling: (1) the `/invites` response is `{data:[…]}` but `signed()` already unwraps the
    envelope — type it as the bare array (fixed in `communities.ts`, don't regress it); (2) the
    `invitableFetchedRef` once-per-address cache needs invalidation — drop the key on fetch failure
    (a transient 500 currently caches "no communities" for the session), remove/refetch after a
    successful invite (else the card re-offers it and the duplicate POST fails silently), and clear
    both `invitable` and the ref on logout/identity change; (3) surface invite errors to the user
    (the bridge currently swallows them with `console.error`); (4) build the submenu on the
    `ContextMenu` primitive instead of the removed bespoke `.submenu`/`.subRow` CSS.
18. **HUD state: `useEngineSession` hook prop-drilled → consider Context / a store** — *architecture,
    low priority*. All HUD state lives in one `useEngineSession` hook at the top of `Hud`, prop-drilled
    down; the returned `session` is a fresh object every render, so the whole HUD re-renders on any
    change. Fine at current scale (engine round-trips are the bottleneck, not React renders), so **not
    urgent**. Nuance if we ever move it: **Context alone doesn't fix re-renders** — a single
    `SessionContext` only removes prop-drilling (ergonomics), because the value changes every render.
    Targeted re-renders (only friends consumers re-render on a friends change) need **memoized slices**
    (the `friends`/`chat` objects are plain literals today) **plus** either split per-domain contexts or
    a **selector store** (zustand/jotai — adds a dep; project is deliberately state-lib-free). Also a
    test cost (harness passes props today; Context needs a provider wrapper). Recommendation: keep
    prop-drilling; add a single `SessionContext` only if drilling ergonomics annoy; memoized slices /
    store only if re-renders become a *measured* problem.
19. **Deep-linkable / bookmarkable navigation — reflect location in the URL** — *architecture, low
    priority*. Entering a scene/world (and, ideally, opening HUD surfaces like the map/backpack) should
    be **parameterized in the URL** so the state is shareable and bookmarkable: reload/paste a URL and
    land in the same realm + coords. Scope to nail down: realm/world + parcel coords (e.g.
    `?realm=…&position=x,y` or a path), whether HUD panels also serialize, and wiring it to the picker
    (`pickDestination`) + `map.teleport`/`changeRealm` so URL ⇄ engine stay in sync (`popstate` → jump,
    jump → `pushState`). Deferred: needs a small router/URL-sync layer (project is router-free today).

20. **Voice feedback — "who's speaking" indicator** — *feature parity, when voice chat is prioritized*.
    The old scene showed an **animated speaking indicator on each avatar nametag** while a nearby player
    talked (and used the local mic state for your own tag). Mechanism to port: the engine exposes a
    voice stream — `BevyApi.getVoiceStream()` yielding `{ sender_address, active }` (`MicActivation`);
    the old scene folded it into a `playerVoiceStateMap[address] = active` and animated a `voice-N`
    sprite on the tag (`bevy-ui-scene`: `components/avatar-tags/avatar-name-tag-3d.tsx` +
    `tag-element.tsx`). For react-web: a **new bridge domain** subscribing to that stream and forwarding
    `{ address, active }` over the channel; session state (a `speaking` set keyed by address); and the
    indicator surfaced on the **engine-rendered nametags** (bridge scene) and/or the DOM **nearby-members
    list** (`chat.members`) — e.g. a pulsing mic ring. Depends on voice chat being wired end-to-end;
    today only the local `mic` toggle exists (no per-remote-speaker signal). Could yield a reusable
    `SpeakingIndicator` primitive.

21. **Migrate inline `dt`-throttle timers to `bridge-scene/src/system-helpers.ts`** — *cleanup*. The
    `throttleByDt` helper (added in PR #915 for `avatarPointer`) replaces a dt-accumulator that's
    re-implemented inline across most bridge domains (`chat`, `world`, `friends`, `project`,
    `nametags` — two timers there —, `avatarPreview`). Migrate them to the helper (and consider
    `singleFlight` / a named `pollSequential` wrapper where a polled async RPC could overlap). Pure
    cleanup, no behavior change; kept out of #915 to stay surgical.

22. **Re-add "Report" to the profile card once a moderation/report endpoint exists** — *feature,
    low priority*. Report was removed from `ProfileCard` in PR #915 because there's no backend — it was
    only a `console.log` stub, so shipping a dead action was worse than hiding it. When a report/
    moderation endpoint lands: re-add the `Report` row + `onBlock`-style `onReport` request prop
    (parent-owned confirm, same pattern as Block), the `ReportIcon` glyph, and wire the actual submit.
    (Old scene logged too — this is genuinely new backend work, not just UI.)

## Not gaps (already good / ahead)

`Modal` (portal + focus-trap + blur + `--ui-scale`, richer than the old backdrop), `IconButton`
(badge + tooltip + shortcut), the **friend-state architecture** (single reactive source, simpler than
the old version-bump), `tokens.css`, and primitives the old lacks (`WearableCard`, `EmptyState`,
`PageHeader`, `CharCounter`, `SearchField`, `ContextMenu`).

## Deliberately NOT ported

- The old Redux `shownPopups` popup-stack registry — React composition + portals already handle modal
  stacking.
- The `friendshipStateVersion` + cached-snapshot + event-bus machinery — an artifact of the SDK7
  per-frame render model; React's targeted re-renders make it unnecessary.

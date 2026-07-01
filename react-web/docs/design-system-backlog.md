# react-web — design-system & architecture backlog

Gaps found by auditing the old system-scene (`~/dev/protocol-squad/bevy-ui-scene`) against
`react-web/src/design`. Ordered by impact. "Shape" = new primitive / extend existing / pattern.

> Context: we're rebuilding the HUD in `react-web` (replacing the in-engine system-scene/scene-ui).
> This list captures what the old UI had that react-web lacks or reimplements bespoke. Some old
> machinery is deliberately **not** ported (see bottom).

## 🔴 High

1. **Toast system** — *new*. Nothing transient/cross-cutting exists. Needed for real-time events
   (remote friend accepted, community invites, item sold…), ephemeral confirmations, and operational
   errors. Today faked with per-component `setTimeout`. (Old: `notification-toast-stack`.)
2. **`Tabs` primitive** — *new*. Tabs are reimplemented bespoke in ~37 files (Settings, Backpack,
   FriendsPanel, CommunityModal…). (Old: `tab-component.tsx`.)
3. **Reusable `FriendButton` + full relationship model** — *new + pattern*. State is already a single
   reactive source ✅, but the add-friend CTA is duplicated per view (ProfileCard, ProfilePassport,
   CommunityModal) with ad-hoc optimism. Need `<FriendButton address>` / `useRelationship`, a
   **6-state** relationship (add `blocked`, `incoming`/Accept to the current 3), centralized optimistic
   update, and **fix CommunityModal desync** (it reads `member.isFriend`, not `session.friends`).
4. **`Button`: `loading` state + `danger`/`destructive` + `link`/text variant** — *extend*. Recurring
   need (jump-in/create/send → loading; unfriend/reject/leave/delete → danger; a subtle underlined
   text-link like the gate's "try anyway…" → link, currently a bespoke `<button>`). (Old:
   `ButtonComponent`.)

## 🟡 Medium

5. **Engine-panic / error capture → popup** — *new* (ACTIVE — see plan). No `ErrorBoundary`, no
   `unhandledrejection`/`window.onerror` handler, no global error surface. The engine WASM can panic
   on launch (e.g. `can't init wasm queue`) and today it only hits the console. Capture engine panics
   + uncaught errors and show the user a popup (message + copy details + reload). (Old: `error-popup`
   + `error-popup-service.showErrorPopup`.)
6. **`useConfirm` / `showAlert` (imperative dialog helpers)** — *new*. `Modal`/`ModalShell` exist but
   each confirm is rebuilt (WorldVisitModal, ExitConfirm). (Old: `confirm-popup` / `alert-popup`.)
7. **`Badge` (standalone)** — *extract*. Badge logic is trapped inside `IconButton`; can't put a badge
   on a tab/avatar/chip without reimplementing. (Old: `notification-badge.tsx`.)
8. **`Chip` / `Tag`** — *new*. "chip" is bespoke in ~11 files (map categories, count pills, status).
   (Old: `color-tag.tsx`.)
9. **Consolidate modals onto `Modal`/`ModalShell`** — *cleanup*. ProfileCard, CommunityModal,
   CommunityCreateModal, WorldVisitModal roll their own portal/overlay and hardcode `z-index: 10001`.
   Unify backdrop / escape / focus-trap / z-layer.
10. **`Radio` / `RadioGroup`** — *new*. Have Checkbox/Toggle/Select but no Radio; bespoke in
    PermissionDialog. (Old: `radio-button.tsx`.)
11. **`Skeleton`** — *new*. Only `Spinner` exists; no load placeholders for lists/cards.
    (Old: `loading-placeholder.tsx`.)

## 🟢 Low / when a feature needs it

12. **`Divider`** (bespoke in ~4 places · old `bottom-border`)
13. **`Pagination`** (unused today · old `pagination/`)
14. **`CopyButton`** (inline in ProfileCard · old `copy-button`)
15. **`Username`** (name + verified · old `player-name-component`)
16. `Button` `iconLeft`/`iconRight` props + `hoverIcon` (niche · old `ButtonComponent`)
17. **HUD state: `useEngineSession` hook prop-drilled → consider Context / a store** — *architecture,
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
18. **Deep-linkable / bookmarkable navigation — reflect location in the URL** — *architecture, low
    priority*. Entering a scene/world (and, ideally, opening HUD surfaces like the map/backpack) should
    be **parameterized in the URL** so the state is shareable and bookmarkable: reload/paste a URL and
    land in the same realm + coords. Scope to nail down: realm/world + parcel coords (e.g.
    `?realm=…&position=x,y` or a path), whether HUD panels also serialize, and wiring it to the picker
    (`pickDestination`) + `map.teleport`/`changeRealm` so URL ⇄ engine stay in sync (`popstate` → jump,
    jump → `pushState`). Deferred: needs a small router/URL-sync layer (project is router-free today).

19. **Voice feedback — "who's speaking" indicator** — *feature parity, when voice chat is prioritized*.
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

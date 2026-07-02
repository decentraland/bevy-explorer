# react-web — backlog

Design-system, architecture, feature-parity, and bug items for the HUD. Ordered by impact within each
priority. Each item is tagged at the start: `[DS]` design-system primitive / extend / cleanup ·
`[feature]` · `[arch]` · `[bug]`. "Shape" = new primitive / extend existing / pattern.

> Context: we're rebuilding the HUD in `react-web` (replacing the in-engine system-scene/scene-ui).
> This list captures what the old UI had that react-web lacks or reimplements bespoke. Some old
> machinery is deliberately **not** ported (see bottom).

## 🔴 High

1. `[bug]` **Enter should focus the chat input (core in-world interaction, currently broken)** —
   *behavior/bug, high impact*. Pressing **Enter** in-world doesn't focus the chat input, so you can't
   start typing the standard way (DCL/game convention: Enter focuses chat → type → Enter sends → Escape
   blurs). The crux: while the engine iframe holds keyboard focus (pointer-locked camera-look) the
   keypress reaches the **engine iframe**, not react-web's DOM, so a plain `keydown` listener never
   sees it. Fix path: capture it globally like `useGlobalHotkey` (already survives engine-iframe focus
   — the FPS toggle uses it) to focus the chat input, **or** have the engine/bridge emit a "focus
   chat" event (the old scene owned chat focus in-engine). Also free the pointer + stop keystrokes
   reaching the engine while typing (so the avatar doesn't move), and restore on blur/Escape. (Old:
   bevy-ui-scene chat Enter-to-focus.)
1b. `[bug]` **HUD hotkeys fire while typing in an SDK7 scene text input** — *behavior/bug, high impact*.
   Typing in a scene-rendered input (search boxes, in-scene forms) triggers the menu shortcuts —
   e.g. pressing **P** opens Settings mid-word. Cause: `useMenuShortcuts` attaches capture-phase
   `keydown` to the **engine iframe window**, and a scene UI input is drawn inside the canvas, so
   `e.target` is the canvas — the `INPUT`/`TEXTAREA`/`isContentEditable` guard never matches.
   Fix path: the HUD needs to know when a scene text input has focus — have the bridge scene relay
   the engine's text-input/IME focus state (a `textInputFocus` message) and suspend `useMenuShortcuts`
   (and any other letter-key hotkeys) while it's true. Same underlying HUD↔engine keyboard-focus
   problem as item 1 — solve them together.
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

## 🟡 Medium

6. `[feature]` **Engine-panic / error capture → popup** — *new* (largely SHIPPED — `ErrorBoundary` +
   `EngineErrorModal` + crash watchdog landed on `fix/react-web-hud`). Kept as a tracking entry: engine
   WASM panics on launch (e.g. `can't init wasm queue`) and runtime crashes now surface a popup
   (message + copy details + reload/dismiss). (Old: `error-popup` + `error-popup-service`.)
7. `[DS]` **`useConfirm` / `showAlert` (imperative dialog helpers)** — *new*. `Modal`/`ModalShell`
   exist but each confirm is rebuilt (WorldVisitModal, ExitConfirm). (Old: `confirm-popup` /
   `alert-popup`.)
8. `[DS]` **`Badge` (standalone)** — *extract*. Badge logic is trapped inside `IconButton`; can't put a
   badge on a tab/avatar/chip without reimplementing. (Old: `notification-badge.tsx`.)
9. `[DS]` **`Chip` / `Tag`** — *new*. "chip" is bespoke in ~11 files (map categories, count pills,
   status). (Old: `color-tag.tsx`.)
10. `[DS]` **Consolidate modals onto `Modal`/`ModalShell`** — *cleanup*. ProfileCard, CommunityModal,
    CommunityCreateModal, WorldVisitModal roll their own portal/overlay and hardcode `z-index: 10001`.
    Unify backdrop / escape / focus-trap / z-layer.
11. `[DS]` **`Radio` / `RadioGroup`** — *new*. Have Checkbox/Toggle/Select but no Radio; bespoke in
    PermissionDialog. (Old: `radio-button.tsx`.)
12. `[DS]` **`Skeleton`** — *new*. Only `Spinner` exists; no load placeholders for lists/cards.
    (Old: `loading-placeholder.tsx`.)
13. `[feature]` **Passport — finish the sections (feature parity with unity-explorer / bevy-ui-scene)**
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

## 🟢 Low / when a feature needs it

14. `[DS]` **`Divider`** (bespoke in ~4 places · old `bottom-border`)
15. `[DS]` **`Pagination`** (unused today · old `pagination/`)
16. `[DS]` **`CopyButton`** (inline in ProfileCard · old `copy-button`)
17. `[DS]` **`Username`** (name + verified · old `player-name-component`)
18. `[DS]` `Button` `iconLeft`/`iconRight` props + `hoverIcon` (niche · old `ButtonComponent`)
18b. `[feature]` **Re-enable "Invite to Community" in `ProfileCard`** — *feature, parked until
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
19. `[arch]` **HUD state: `useEngineSession` hook prop-drilled → consider Context / a store** —
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
    memoized slices / store only if re-renders become a *measured* problem.
20. `[arch]` **Deep-linkable / bookmarkable navigation — reflect location in the URL** — *architecture,
    low priority*. Entering a scene/world (and, ideally, opening HUD surfaces like the map/backpack)
    should be **parameterized in the URL** so the state is shareable and bookmarkable: reload/paste a
    URL and land in the same realm + coords. Scope to nail down: realm/world + parcel coords (e.g.
    `?realm=…&position=x,y` or a path), whether HUD panels also serialize, and wiring it to the picker
    (`pickDestination`) + `map.teleport`/`changeRealm` so URL ⇄ engine stay in sync (`popstate` → jump,
    jump → `pushState`). Deferred: needs a small router/URL-sync layer (project is router-free today).

21. `[feature]` **Voice feedback — "who's speaking" indicator** — *feature parity, when voice chat is
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

22. `[feature]` **Passport edit mode (own profile)** — *feature, own-profile only*. bevy-ui-scene lets
    you edit your own passport in place — About Me, the info-field dropdowns, links (add/remove, up to
    5), and display name — then deploys the updated profile. react-web's passport is read-only today.
    Larger than the view-parity item (#13) — needs edit inputs + a profile-deploy path over the bridge —
    hence separate and lower priority than showing OTHER users' passports correctly.

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

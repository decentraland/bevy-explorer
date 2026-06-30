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
4. **`Button`: `loading` state + `danger`/`destructive` variant** — *extend*. Recurring need
   (jump-in/create/send → loading; unfriend/reject/leave/delete → danger). (Old: `ButtonComponent`.)

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

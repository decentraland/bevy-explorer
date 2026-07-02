# review.md — how we keep the HUD working

The single place to look before merging anything in `react-web/`. It defines the **test harness**
(what proves "everything still works"), the **per-domain expectations**, and the **pre-merge
checklist**. Keep it current: when you add a domain or a rule, add it here.

---

## 0. TL;DR — run this before you push

```bash
# from react-web/
npm run typecheck          # tsc, both the app and (separately) the bridge scene
npm test                   # tier 1 — 140+ vitest domain/component tests (fast, deterministic)
npm run test:visual        # tier 1.5 — visual regression of every DOM domain (headless, no engine)
(cd bridge-scene && npx eslint "src/**/*.{ts,tsx}")
```

If `test:visual` reports diffs, open the Playwright HTML report (`npx playwright show-report`) and
look at the **expected vs actual vs diff** images. Either it's a real regression (fix it) or an
intended change (refresh the baseline — see §2).

The real engine + the world-space UI it draws (3D nametags, crosshair) can't be screenshotted
deterministically — they have their own tiers (§3, §4).

---

## 1. The testing tiers (why three)

| Tier | Command | What it proves | Engine? | Deterministic? |
|------|---------|----------------|---------|----------------|
| **1 — contract** | `npm test` | Every page→scene API call posts the exact wire message, and every scene→page message updates state. Component clicks fire the right action. | no (FakeDriver) | yes |
| **1.5 — visual** | `npm run test:visual` | Every DOM **domain renders correctly** — layout, colour, tokens, spacing — diffed against a committed baseline PNG. | no (`?mock=1`) | yes |
| **2 — engine** | `npm run test:e2e` | The guest-reachable calls actually **round-trip through the real bevy engine + bridge scene**. | yes (headed, WebGPU) | partly |

Tier 1 is the contract net (every call, including ones a guest can't reach). Tier 1.5 is the
*appearance* net (the thing humans notice). Tier 2 is the *it-actually-talks-to-the-engine* net.
They're complementary; none replaces another.

> **Why visual regression over an "agent drives Chrome and looks" approach?** A committed-baseline
> Playwright suite is repeatable, runs in CI, pins the exact pixels, and needs no human/agent in the
> loop to pass. The agent-driven flow is great for *exploring* a new bug, but it can't be a
> regression gate. So: Playwright for the gate, agent checklist (§4) only for what can't be mocked.

---

## 2. Tier 1.5 — the visual harness (`e2e/visual.spec.ts`)

Renders the HUD in **mock mode** (`?mock=1`: fake bridge, fixed data, no engine/GPU) and screenshots
each domain. Config: `playwright.visual.config.ts` (headless, 1600×900, `maxDiffPixelRatio: 0.01`).

**Determinism is engineered, not hoped for** (`prepare()` in the spec):
- **Frozen clock** (`page.clock.setFixedTime`) → relative times ("2h ago", "Yesterday") are stable.
- **External images stubbed** → every non-localhost avatar/thumbnail becomes a 1×1 transparent PNG,
  so the network can't make a screenshot flake (layout is preserved; only the bitmap is blanked).
- **Animations disabled** at capture time; **fixed viewport + `deviceScaleFactor: 1`**.

**Domains covered** (one baseline each, in `e2e/visual.spec.ts-snapshots/`):
`showcase` · `login-fresh` · `login-welcome` · `world-hud` · `panel-friends` · `panel-settings` ·
`panel-profile` · `panel-notifications` · `panel-emote-wheel` · `panel-communities` · `panel-map` ·
`backpack-wearables` · `backpack-emotes`.

**Updating baselines** (only when the change is intentional):
```bash
npm run test:visual:update      # regenerates the PNGs
git add e2e/visual.spec.ts-snapshots && git diff --cached --stat   # then EYEBALL the new PNGs
```
Never blind-update. A baseline refresh in a PR must be reviewed image-by-image.

**Caveats / known limits**
- **Baselines are platform-specific** (`*-chromium-darwin.png`). Font hinting differs across OSes, so
  generate/verify on the same platform CI uses (or add a Linux baseline set when CI is wired).
- **Mock data is the ceiling.** A panel can only be as rich as `src/engine/mockBridge.ts` makes it.
  - **`backpack-emotes` renders empty in the baseline** — even though the Backpack *does* request
    `getEmotes` on open and the mock returns 10 base emotes. *(Surfaced by this harness — to chase
    down: is it a render/timing nuance in the capture, or does `emotes.list` not reach the grid?)*
  - Enrich mock `getEmotes` with an OWNED collection (some with `slot`, varied `rarity`/`count`) so
    the new owned-grid + assign-to-slot are exercised, not just the 10 base emotes.

---

## 3. Tier 2 — real engine (`e2e/engine.spec.ts` + `e2e/engine-clicks.spec.ts`)

See `e2e/README.md`. Needs a real GPU + a local engine build (`../deploy/web`). Run before a release
or when touching the bridge protocol.

- **`engine.spec.ts`** — every panel *opens* and *fetches* (the call crosses the bridge).
- **`engine-clicks.spec.ts`** — the controls *inside* panels do something the **engine** sees,
  verified **through the bevy console API** where the engine exposes state, bridge-spy otherwise:

  | Click | Verified by |
  |-------|-------------|
  | move / teleport | `/player_position` lands at the exact coords (`helpers.position`) |
  | equip a wearable | `/get_user_data` profile **`vN` bumps** (`helpers.profileVersion`) + `equip` on the bridge |
  | assign an emote to a slot | `equipEmote` on the bridge |
  | play a wheel emote | `triggerEmote` on the bridge |
  | change a setting (slider) | `setSetting` on the bridge |
  | (sanity) profile + roster reachable | `/get_user_data`, `/connected_players` |

  Console helpers live in `e2e/helpers.ts` (`position`, `getUserData`, `profileVersion`,
  `connectedPlayers`). Available bevy commands: `move_player_to`, `walk_player_to`, `teleport`,
  `player_position`, `get_user_data`, `connected_players`, `live_scenes`, `list_portables`, `emote`,
  plus `/login_*`, `/logout`, `/chat`.

**Still tier-1-only** (a fresh guest can't reach them, so they can't round-trip here): accepting a
friend request, leaving a community, marking notifications read — these need seeded data and are
covered deterministically in tier 1. **TODO to deepen tier 2** (would need engine query commands or
fixtures): map *UI* teleport by clicking a parcel, a `/mic_state` query to confirm the mic toggle,
logout via the profile chip.

---

## 4. World-space UI — agent / manual checklist (can't be mocked)

The 3D nametags, the pointer-lock crosshair, and projected proximity tips are drawn **in the engine
scene**, not the DOM, so `?mock=1` can't show them and screenshots can't be deterministic. Verify
these by hand (or have an agent drive the Chrome extension against a live world) after any change to
`bridge-scene/src/domains/{nametags,pointer,proximity,avatarPointer}.*`:

- [ ] **Nametags** appear above every avatar's head, follow them while walking, and **face the camera**.
- [ ] Name **colour** = profile custom colour if set, else the address-hash palette; **claimed** names
      show the verified seal, **unclaimed** show `#abcd` tight to the name.
- [ ] **Fixed world scale — never a giant.** A tag is a constant *world* size (it shrinks with distance,
      like the old SDK scene); it must NEVER balloon into a screen-filling pill when the camera pulls back
      or when its `AvatarAttach` fails to bind. (We dropped the old constant-on-screen scaling precisely
      because it turned a stranded tag into a giant.)
- [ ] **Exactly one tag per address — no duplicates / orphans / pile-up.** None accumulate over a long
      session, after alt-tabbing away and back, or across a comms/livekit blip's leave→enter churn (the
      classic regression). They're pooled — one reusable entity per wallet, never destroyed (engine entity
      removal is unreliable for attached entities; we show/hide instead).
- [ ] Tags **fade out** past ~20–40 m. Your **own** tag shows above your head in third person and is
      hidden in first person.
- [ ] **Crosshair** shows when the camera is locked (mouse hidden) and hides when the cursor is free.
- [ ] **Hover / proximity prompts** ("Press E…") show on interactables and sit on the right entity.
      Free-cursor prompts sit **around the mouse** in radial slots (from `PrimaryPointerInfo`) — first
      to the right, then left, then top, up to 7 (mirrors bevy-ui-scene); pointer-locked falls back to
      a stack under the reticle. A **disabled** prompt swaps its key-cap and copy by which
      `PBPointerEvents` distance rule gates it: **camera** (no `maxPlayerDistance` configured — the
      common case, incl. the implicit 10m default) shows a camera glyph + "Get camera closer";
      **player** (`maxPlayerDistance` configured) shows the bevy-ui-scene walking glyph + "Get player
      closer". (Visual baseline via `?mock=1&simhover=N`.)
- [ ] **Nearby-avatar click → profile card** (`domains/avatarPointer.ts`) — aiming at another avatar
      shows a **"Show Profile"** hover; clicking it frees the cursor and opens that avatar's
      **profile card** anchored **at the click point**. The card mirrors the old profile-menu action
      set: friend CTA (Add / Accept+Reject / Requested), **View Passport** (→ passport-popup),
      **Mention** (drops @name into chat), **Invite to Community** (owner/mod only — expands the list),
      **Block/Unblock** (confirm), **Report** (confirm). Walking a player out of / back into range removes/re-adds
      the click; the card dismisses on scrim-click or an action. (Report has no endpoint yet — stub.)
      Your own avatar is never clickable (third person). Standing in a **DISABLE_PASSPORTS** privacy
      area hides the profile card on click for nearby avatars (`BevyApi.getAvatarModifiers`).

---

## 5. Pre-merge review checklist

Run through this on every diff (it encodes `AGENTS.md`):

- [ ] **Design system** — no new hardcoded brand/status hex, `rgba()`, radius, or type size in a
      component (`grep` your diff for `#` and `rgba(`); each must be a `tokens.css` var or justified.
      No bespoke `<button>`/control where a `src/design/` primitive exists.
- [ ] **No dead code** — no unused exports/CSS classes/imports; no commented-out blocks; comments are
      sparse and explain *why*, not *what*.
- [ ] **Structure** — one concern per file; reuse a primitive or *create* one rather than inlining a
      recurring pattern; bridge logic in one `domains/*` file.
- [ ] **Perf** — catalyst fetches cached (per address); per-frame engine systems are change-gated /
      throttled, not doing work every frame for free.
- [ ] **Tests** — tier 1 still green; **a new/changed domain has a tier-1.5 baseline**; world-space
      changes ran the §4 checklist.
- [ ] **Both projects typecheck** (`npm run typecheck` + `cd bridge-scene && npx tsc --noEmit`).
- [ ] **No secrets / `.env`** staged; commit follows `CLAUDE.md` (imperative, no AI attribution).

---

## 6. Adding a domain

1. Build it (token-driven, reusing `src/design/` primitives).
2. Add the mock data in `src/engine/mockBridge.ts` so `?mock=1` renders it richly.
3. Add a tier-1 test (`src/test/<domain>.test.tsx`) for its API contract + clicks.
4. Add a screenshot to `e2e/visual.spec.ts` and run `npm run test:visual:update`.
5. If it's world-space (engine-drawn), add a line to the §4 checklist instead of a screenshot.
6. List its expectations here.

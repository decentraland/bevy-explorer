// Tier 1.5 — VISUAL REGRESSION over the mock HUD (`?mock=1`). One screenshot per DOM domain,
// diffed against a committed baseline. Deterministic by construction (see `prepare`): frozen clock,
// stubbed external images, disabled animations, fixed viewport — so a diff means a real visual
// change, not flakiness. Run with `npm run test:visual`; refresh baselines with
// `npm run test:visual:update` (and eyeball the new PNGs before committing). World-space UI
// (3D nametags, crosshair) can't be mocked — it's covered by the agent checklist in review.md.
import { test, expect, type Page } from '@playwright/test'

// A fixed instant so every relative timestamp ("2h ago", "Yesterday") renders identically each run.
const FIXED_TIME = new Date('2025-06-26T15:00:00Z')
// 1×1 transparent PNG — every external avatar/thumbnail is stubbed to this, so screenshots never
// depend on the network and external image churn can't cause false diffs. Layout is preserved.
const BLANK_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64'
)

const SHOP_FIXTURE = [
  { id: 's1', name: 'Neon Tiara', thumbnail: 'https://example.com/s1.png', rarity: 'epic', category: 'wearable', url: '/contracts/0x1/items/1', isOnSale: true, price: '2500000000000000000' },
  { id: 's2', name: 'Pixel Jacket', thumbnail: 'https://example.com/s2.png', rarity: 'rare', category: 'wearable', url: '/contracts/0x2/items/0', isOnSale: false, price: '0', minListingPrice: '12000000000000000000' },
  { id: 's3', name: 'Free Cap', thumbnail: 'https://example.com/s3.png', rarity: 'common', category: 'wearable', url: '/contracts/0x3/items/2', isOnSale: true, price: '0' }
]

const EVENTS_FIXTURE = [
  { id: 'e1', name: 'Genesis Plaza party', x: 0, y: 0, live: true, start_at: '2025-06-26T14:00:00Z', total_attendees: 12, image: 'https://example.com/e1.png' },
  { id: 'e2', name: 'Galaga night', x: 0, y: 0, world: true, server: 'galaga.dcl.eth', live: true, start_at: '2025-06-26T14:30:00Z', total_attendees: 3 }
]

const PLACES_FIXTURE = [
  { id: 'p1', title: 'Genesis Plaza', image: 'https://example.com/p1.png', positions: ['0,0'], base_position: '0,0', owner: null, contact_name: 'Decentraland Foundation', user_count: 8 },
  { id: 'p2', title: 'Old McTiger Farm', image: 'https://example.com/p2.png', positions: ['10,20'], base_position: '10,20', owner: null, contact_name: 'METATIGER', user_count: 3 },
  { id: 'p3', title: 'Antrom RPG', image: 'https://example.com/p3.png', positions: ['-30,40'], base_position: '-30,40', owner: null, contact_name: 'Matt', user_count: 0 },
  { id: 'p4', title: 'Sky Chaser', image: 'https://example.com/p4.png', positions: ['55,-12'], base_position: '55,-12', owner: null, contact_name: 'stom', user_count: 5 }
]

/** The lobby's places, worlds and live events, fixed. */
async function stubLobbyData(page: Page): Promise<void> {
  const places = (data: unknown[]): { contentType: string; body: string } => ({ contentType: 'application/json', body: JSON.stringify({ ok: true, total: data.length, data }) })
  await page.route(/places\.[^/]+\/api\/places\?with_realms_detail=true&positions=/, (route) => route.fulfill(places(PLACES_FIXTURE.slice(0, 1))))
  await page.route(/places\.[^/]+\/api\/(places|destinations|worlds)/, (route) => route.fulfill(places(PLACES_FIXTURE)))
  await page.route(/worlds-content-server\.[^/]+\/live-data/, (route) => route.fulfill({ contentType: 'application/json', body: JSON.stringify({ data: { perWorld: [] } }) }))
  await page.route(/\/api\/events\?with_connected_users=true/, (route) => route.fulfill({ contentType: 'application/json', body: JSON.stringify({ ok: true, data: EVENTS_FIXTURE }) }))
}

/** Make the page deterministic — call before the first navigation in each test. */
async function prepare(page: Page): Promise<void> {
  // install (not setFixedTime): setFixedTime pins Date but leaves setTimeout on REAL time, so the
  // mock bridge's staggered delivery (roster 1.4s, friends 1.6s, chat drip ≤9.1s — mockBridge
  // spawnPlayer) raced the screenshot and pass/fail depended on machine load. install virtualizes
  // the timers too, letting settle() fast-forward every test to the same terminal state.
  await page.clock.install({ time: FIXED_TIME })
  await page.route(/^https?:\/\/(?!localhost|127\.0\.0\.1)/, (route) => {
    const type = route.request().resourceType()
    if (type === 'image' || type === 'media') return route.fulfill({ contentType: 'image/png', body: BLANK_PNG })
    return route.continue()
  })
  // Live events data changes by the minute; serve a fixed list (sidebar badge + Events page).
  await page.route(/marketplace-api\.[^/]+\/v1\/catalog/, (route) => route.fulfill({ contentType: 'application/json', body: JSON.stringify({ data: SHOP_FIXTURE, total: SHOP_FIXTURE.length }) }))
  await page.route(/\/api\/events\?list=/, (route) => route.fulfill({ contentType: 'application/json', body: JSON.stringify({ ok: true, data: EVENTS_FIXTURE }) }))
}

/** Fonts loaded + a beat for layout to settle (animations are frozen at screenshot time anyway). */
async function settle(page: Page): Promise<void> {
  await page.evaluate(() => document.fonts.ready.then(() => undefined))
  // Jump virtual time past the mock bridge's last staggered timer (chat drip ends at ~9.1s) so
  // every screenshot captures the same fully-delivered state, regardless of wall-clock timing.
  await page.clock.fastForward(15_000)
  await page.waitForTimeout(200)
}

/** Leave the lobby through its landing card (home) — the first JUMP IN on the page, enabled once
 *  sign-in has finished. */
async function jumpInFromLobby(page: Page): Promise<void> {
  await page.getByRole('heading', { name: /Welcome/ }).waitFor()
  await page.getByRole('button', { name: /^jump in$/i }).first().click()
}

async function enterWorld(page: Page): Promise<void> {
  await page.goto('/?mock=1')
  await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).click()
  await jumpInFromLobby(page)
  await page.waitForSelector('nav[aria-label="Main navigation"]')
}

/** Enter as the returning mock user (`previousLogin=1`): a wallet holding two claimed NAMEs, which
 *  is what the name editor's picker and tabs need in order to appear. */
async function enterWorldReturning(page: Page): Promise<void> {
  await page.goto('/?mock=1&previousLogin=1')
  await page.getByRole('button', { name: /JUMP INTO DECENTRALAND/i }).click()
  await jumpInFromLobby(page)
  await page.waitForSelector('nav[aria-label="Main navigation"]')
}

// The map isn't on the rail: reach it from another menu page's top bar.
async function openPanel(page: Page, label: string): Promise<void> {
  if (label !== 'Map') return await page.getByRole('button', { name: label, exact: true }).click()
  await page.getByRole('button', { name: 'Places', exact: true }).click()
  await page.locator('[data-page="map"]').click()
}

test.describe('visual — mock HUD', () => {
  test.beforeEach(async ({ page }) => {
    await prepare(page)
  })

  test('design-system showcase', async ({ page }) => {
    await page.goto('/?showcase=1')
    await settle(page)
    await expect(page).toHaveScreenshot('showcase.png', { fullPage: true })
  })

  test('login — fresh (sign in or guest)', async ({ page }) => {
    await page.goto('/?mock=1')
    await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('login-fresh.png')
  })

  test('login — welcome back', async ({ page }) => {
    await page.goto('/?mock=1&previousLogin=1')
    await settle(page)
    await expect(page).toHaveScreenshot('login-welcome.png')
  })

  // Mobile gate — the download-the-app page shown on mobile (forced with ?gate=1; desktop UA → both
  // store buttons). Returns before the HUD, so no ?mock needed.
  test('lobby', async ({ page }) => {
    await stubLobbyData(page)
    await page.goto('/?mock=1&previousLogin=1')
    await page.getByRole('button', { name: /JUMP INTO DECENTRALAND/i }).click()
    await page.getByRole('heading', { name: /Welcome/ }).waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('lobby.png')
  })

  test('mobile gate', async ({ page }) => {
    await page.goto('/?gate=1')
    await settle(page)
    await expect(page).toHaveScreenshot('mobile-gate.png')
  })

  // Browser gate — the "use Chrome" page shown on non-Chromium desktop (forced with ?gate=browser).
  test('browser gate', async ({ page }) => {
    await page.goto('/?gate=browser')
    await settle(page)
    await expect(page).toHaveScreenshot('browser-gate.png')
  })

  // GPU gate — the "enable WebGPU / hardware acceleration" page shown before boot when no usable GPU
  // adapter is found (forced with ?gate=gpu; real detection is the async probe in App).
  test('gpu gate', async ({ page }) => {
    await page.goto('/?gate=gpu')
    await settle(page)
    await expect(page).toHaveScreenshot('gpu-gate.png')
  })

  // Engine error popup — ?simerror=launch seeds a sample boot-panic (fatal: Reload only, no
  // Dismiss). Mock mode → no engine iframe, fully deterministic.
  test('engine error popup', async ({ page }) => {
    await page.goto('/?mock=1&simerror=launch')
    await settle(page)
    await expect(page).toHaveScreenshot('engine-error.png')
  })

  // Realm error popup — ?simerror=realm seeds a "world not found" fatalError, distinct from the
  // full-screen CrashModal above: it renders through the popup layer (dismissible, PopupHost .dim).
  test('realm error popup', async ({ page }) => {
    await page.goto('/?mock=1&simerror=realm')
    await settle(page)
    await expect(page).toHaveScreenshot('realm-error.png')
  })

  // Passport — the full profile popup opened from the profile card's "View Passport".
  test('passport', async ({ page }) => {
    await enterWorld(page)
    await page.getByRole('button', { name: 'View Sharknado' }).first().click()
    await page.getByRole('button', { name: 'View Passport' }).click()
    await settle(page)
    await expect(page).toHaveScreenshot('passport.png')
  })

  // Permission dialog — ?perm=1 fires a sample scene permission request shortly after entering world.
  test('permission dialog', async ({ page }) => {
    await page.goto('/?mock=1&perm=1')
    await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).click()
    await jumpInFromLobby(page)
    await page.getByRole('alertdialog').waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('permission-dialog.png')
  })

  // Community modal — opened by clicking a community card from the Communities panel.
  test('community modal', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Communities')
    await page.getByRole('button', { name: /Decentraland Foundation/ }).first().click()
    await page.getByRole('heading', { name: 'Decentraland Foundation', level: 2 }).waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('community-modal.png')
  })

  // Community create modal — "+ CREATE A COMMUNITY" from the Communities panel.
  test('community create modal', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Communities')
    await page.getByRole('button', { name: /CREATE A COMMUNITY/i }).click()
    await page.getByRole('dialog').waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('community-create-modal.png')
  })

  test('world HUD (sidebar + chat)', async ({ page }) => {
    await enterWorld(page)
    await settle(page)
    await expect(page).toHaveScreenshot('world-hud.png')
  })

  // In-world loading screen (tips carousel), driven by a scene-loading update on the mock's channel.
  test('loading screen', async ({ page }) => {
    await enterWorld(page)
    const loadingUpdate = (pendingAssets: number): Promise<void> =>
      page.evaluate((n) => {
        const ch = new BroadcastChannel(`bevy-ui-bridge#${(window as { __bridgeSession?: string }).__bridgeSession}`)
        ch.postMessage({ to: 'page', msg: { kind: 'sceneLoading', state: { visible: true, realmConnected: true, title: '', pendingAssets: n } } })
      }, pendingAssets)
    await loadingUpdate(30)
    await page.getByRole('status').filter({ hasText: 'LOADING 10%' }).waitFor()
    await loadingUpdate(9)
    await page.getByRole('status').filter({ hasText: 'LOADING 59%' }).waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('loading-screen.png')
  })

  // The Shop section, reached from the menu bar (it has no sidebar button, like Unity's Explore panel).
  test('shop', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Events')
    await page.getByRole('button', { name: /^Shop/ }).click()
    await page.getByText('Neon Tiara').waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('shop.png')
  })

  // Element-level with a fixed pixel budget: 1% of this thin strip would hide a whole icon change.
  test('sidebar', async ({ page }) => {
    await enterWorldReturning(page)
    await settle(page)
    await expect(page.locator('nav[aria-label="Main navigation"]')).toHaveScreenshot('sidebar.png', { maxDiffPixels: 20 })
  })

  // Profile card — the popover opened by clicking a chat sender / nearby avatar. Baselines the
  // action set (View Passport · Mention · Block). The block confirm and the relationship
  // states (Accept/Reject/Unblock) are covered deterministically by the tier-1 profileCard.test.tsx.
  test('profile card', async ({ page }) => {
    await enterWorld(page)
    await page.getByRole('button', { name: 'View Sharknado' }).first().click()
    const card = page.getByRole('dialog', { name: 'Profile' })
    await card.getByRole('button', { name: 'Block' }).waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('profile-card.png')
  })

  // Radial free-cursor hover tooltips around the pointer (up to 7 slots), ported from the old scene.
  // ?simhover=7 seeds seven prompts (one disabled → "Too far, get closer"); React anchors them at the
  // live DOM cursor, so we move the mouse to the viewport centre to place them deterministically.
  test('hover tooltips (radial)', async ({ page }) => {
    await page.goto('/?mock=1&simhover=7')
    await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).click()
    await jumpInFromLobby(page)
    await page.waitForSelector('nav[aria-label="Main navigation"]')
    await page.getByText('Show Profile').waitFor() // the seeded hover arrives ~1.5s after entry
    const vp = page.viewportSize()
    if (vp) await page.mouse.move(vp.width / 2, vp.height / 2)
    await settle(page)
    await expect(page).toHaveScreenshot('hover-tooltips.png')
  })

  // Floating panels + full-screen pages, opened from the sidebar.
  for (const [label, name] of [
    ['Friends', 'friends'],
    ['Settings', 'settings'],
    ['Profile', 'profile'],
    ['Notifications', 'notifications'],
    ['Emotes', 'emote-wheel'],
    ['Communities', 'communities'],
    ['Map', 'map'],
    ['Events', 'events'],
    ['Skybox', 'skybox']
  ] as const) {
    test(`panel — ${name}`, async ({ page }) => {
      await enterWorld(page)
      await openPanel(page, label)
      await settle(page)
      await expect(page).toHaveScreenshot(`panel-${name}.png`)
    })
  }

  // Your own passport in edit mode — reached by the pencil on the About card; SAVE and CANCEL take
  // over the header, the tab bar stands down and the form replaces the card.
  test('passport — edit mode', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Profile')
    await page.getByRole('button', { name: 'Edit profile' }).click()
    await settle(page)
    await expect(page).toHaveScreenshot('passport-edit.png')
  })

  // The name editor, opened by the pencil beside the name. The returning user owns NAMEs, so the
  // tabs, the picker and the upsell panel are all on screen.
  test('name editor', async ({ page }) => {
    await enterWorldReturning(page)
    await openPanel(page, 'Profile')
    await page.getByRole('button', { name: 'Edit name' }).click()
    await page.getByRole('dialog').waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('name-edit.png')
  })

  // The Key Bindings tab inside Settings: chip rows, pair/quad boxes, the fixed wheel chips —
  // rendered from the mock's default binding table.
  test('panel — settings key bindings', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Settings')
    await page.getByRole('tab', { name: 'Key Bindings', exact: true }).click()
    await settle(page)
    await expect(page).toHaveScreenshot('panel-settings-keybindings.png')
  })

  // Minimap — the HUD's newest surface, and the one `world-hud.png` covers worst: it is mostly
  // dark chrome over a dark HUD, so a whole missing minimap only moved ~6k pixels there, inside
  // the 1% tolerance. Baselined with the gear menu open, which is the densest state (header,
  // zoom, gear, and the three menu sections). Map tiles are external images and don't load here,
  // so this baselines the chrome, not the imagery.
  test('minimap (settings open)', async ({ page }) => {
    await enterWorld(page)
    await page.getByRole('button', { name: 'Minimap settings' }).click()
    await page.getByRole('menu', { name: 'Minimap settings' }).waitFor()
    await settle(page)
    await expect(page).toHaveScreenshot('minimap-settings.png')
  })

  test('backpack — wearables', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Backpack')
    await settle(page)
    await expect(page).toHaveScreenshot('backpack-wearables.png')
  })

  test('backpack — EQUIP stays reachable when moving from a card down to it', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Backpack')
    await settle(page)
    const card = page.locator('[data-rarity][aria-pressed]').first()
    const box = await card.boundingBox()
    if (box == null) throw new Error('no card')
    const x = box.x + box.width / 2
    for (let y = box.y + box.height / 2; y <= box.y + box.height + 30; y += 3) await page.mouse.move(x, y)
    const under = await page.evaluate(([px, py]) => document.elementFromPoint(px, py)?.textContent ?? '', [x, box.y + box.height + 30])
    expect(under).toMatch(/^(UN)?EQUIP$/)
  })

  test('sidebar — icons draw at the reference sizes', async ({ page }) => {
    await enterWorld(page)
    const iconWidth = (name: string): Promise<number> =>
      page.getByRole('button', { name, exact: true }).evaluate((b) => (b.querySelector('span[aria-hidden]')?.getBoundingClientRect().width ?? 0) / b.getBoundingClientRect().width * 32)
    expect(await iconWidth('Notifications')).toBeCloseTo(30, 0)
    expect(await iconWidth('Backpack')).toBeCloseTo(28, 0)
    expect(await iconWidth('Help & Support')).toBeCloseTo(32, 0)
    expect(await iconWidth('Chat')).toBeCloseTo(26, 0)
  })

  test('sidebar — the minimap covers neither the tooltips nor the "..." panel', async ({ page }) => {
    await enterWorld(page)
    // Tooltips ignore the pointer, so compare stacking: the rail must sit above the minimap.
    const z = await page.evaluate(() => {
      const zOf = (el: Element | null | undefined): number => Number(el != null ? getComputedStyle(el).zIndex : 0)
      const minimap = document.querySelector('[aria-label="Minimap zoom"]')?.closest('[class*="root"]')
      return { rail: zOf(document.querySelector('nav[aria-label="Main navigation"]')), minimap: zOf(minimap) }
    })
    expect(z.rail).toBeGreaterThan(z.minimap)
    await page.getByRole('button', { name: 'Sidebar settings' }).click()
    const panelOnTop = await page.getByRole('dialog', { name: 'Sidebar settings' }).evaluate((el) => {
      const r = el.getBoundingClientRect()
      return el.contains(document.elementFromPoint(r.right - 20, r.top + r.height / 2))
    })
    expect(panelOnTop).toBe(true)
  })

  test('backpack — emotes', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Backpack')
    await page.getByRole('tab', { name: 'Emotes', exact: true }).click()
    await settle(page)
    await expect(page).toHaveScreenshot('backpack-emotes.png')
  })
})

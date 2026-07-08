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

/** Make the page deterministic — call before the first navigation in each test. */
async function prepare(page: Page): Promise<void> {
  // install (not setFixedTime): setFixedTime pins Date but leaves setTimeout on REAL time, so the
  // mock bridge's staggered delivery (roster 1.4s, friends 1.6s, chat drip ≤6.9s — mockBridge
  // spawnPlayer) raced the screenshot and pass/fail depended on machine load. install virtualizes
  // the timers too, letting settle() fast-forward every test to the same terminal state.
  await page.clock.install({ time: FIXED_TIME })
  await page.route(/^https?:\/\/(?!localhost|127\.0\.0\.1)/, (route) => {
    const type = route.request().resourceType()
    if (type === 'image' || type === 'media') return route.fulfill({ contentType: 'image/png', body: BLANK_PNG })
    return route.continue()
  })
}

/** Fonts loaded + a beat for layout to settle (animations are frozen at screenshot time anyway). */
async function settle(page: Page): Promise<void> {
  await page.evaluate(() => document.fonts.ready.then(() => undefined))
  // Jump virtual time past the mock bridge's last staggered timer (chat drip ends at ~6.9s) so
  // every screenshot captures the same fully-delivered state, regardless of wall-clock timing.
  await page.clock.fastForward(15_000)
  await page.waitForTimeout(200)
}

async function enterWorld(page: Page): Promise<void> {
  await page.goto('/?mock=1')
  await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).click()
  // Entry now goes through the destination picker; skip it (default spawn) to reach the world HUD.
  await page.getByRole('button', { name: /SKIP TO GENESIS PLAZA/i }).click()
  await page.waitForSelector('nav[aria-label="Main navigation"]')
}

const openPanel = (page: Page, label: string): Promise<void> =>
  page.getByRole('button', { name: label, exact: true }).click()

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

  // Engine error popup — ?simerror=launch seeds a sample boot-panic (fatal: Reload + Copy, no
  // Dismiss). Mock mode → no engine iframe, fully deterministic.
  test('engine error popup', async ({ page }) => {
    await page.goto('/?mock=1&simerror=launch')
    await settle(page)
    await expect(page).toHaveScreenshot('engine-error.png')
  })

  test('world HUD (sidebar + chat)', async ({ page }) => {
    await enterWorld(page)
    await settle(page)
    await expect(page).toHaveScreenshot('world-hud.png')
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
    await page.getByRole('button', { name: /SKIP TO GENESIS PLAZA/i }).click()
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
    ['Map', 'map']
  ] as const) {
    test(`panel — ${name}`, async ({ page }) => {
      await enterWorld(page)
      await openPanel(page, label)
      await settle(page)
      await expect(page).toHaveScreenshot(`panel-${name}.png`)
    })
  }

  test('backpack — wearables', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Backpack')
    await settle(page)
    await expect(page).toHaveScreenshot('backpack-wearables.png')
  })

  test('backpack — emotes', async ({ page }) => {
    await enterWorld(page)
    await openPanel(page, 'Backpack')
    await page.getByRole('button', { name: 'Emotes', exact: true }).click()
    await settle(page)
    await expect(page).toHaveScreenshot('backpack-emotes.png')
  })
})

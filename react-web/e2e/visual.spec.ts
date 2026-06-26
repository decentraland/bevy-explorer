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
  await page.clock.setFixedTime(FIXED_TIME)
  await page.route(/^https?:\/\/(?!localhost|127\.0\.0\.1)/, (route) => {
    const type = route.request().resourceType()
    if (type === 'image' || type === 'media') return route.fulfill({ contentType: 'image/png', body: BLANK_PNG })
    return route.continue()
  })
}

/** Fonts loaded + a beat for layout to settle (animations are frozen at screenshot time anyway). */
async function settle(page: Page): Promise<void> {
  await page.evaluate(() => document.fonts.ready.then(() => undefined))
  await page.waitForTimeout(200)
}

async function enterWorld(page: Page): Promise<void> {
  await page.goto('/?mock=1')
  await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).click()
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

  test('world HUD (sidebar + chat)', async ({ page }) => {
    await enterWorld(page)
    await settle(page)
    await expect(page).toHaveScreenshot('world-hud.png')
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

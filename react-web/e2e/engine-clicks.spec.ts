// Tier 2 — deeper IN-PANEL click coverage. engine.spec.ts proves each panel OPENS + fetches; this
// proves the controls INSIDE them do something the ENGINE actually sees. Verified THROUGH THE BEVY
// CONSOLE API wherever the engine exposes the state (player_position after a move, get_user_data
// `vN` after an equip), and through the bridge spy for the message contract otherwise.
//
// Needs a real GPU + the bridge scene on :8100 — see e2e/README.md. Run: `npm run test:e2e`.
// (Selectors here drive the real DOM; if the engine build shifts a control, tune the locator — the
// console assertions are the durable part.)
import { test, expect, type Page } from '@playwright/test'
import {
  enterAsGuest,
  sidebar,
  expectBridge,
  movePlayerTo,
  position,
  getUserData,
  profileVersion,
  connectedPlayers
} from './helpers'

test.describe.configure({ mode: 'serial' })

test.describe('react HUD ↔ engine — in-panel clicks (console-verified)', () => {
  let page: Page
  test.beforeAll(async ({ browser }) => {
    page = await browser.newPage()
    await enterAsGuest(page)
  })
  test.afterAll(async () => {
    await page.close()
  })

  // --- the console state queries are live + describe a real guest --------------
  test('console: get_user_data returns the guest profile (address + version)', async () => {
    const data = await getUserData(page)
    expect(data).toMatch(/0x[0-9a-fA-F]/) // wallet address
    expect(data).toMatch(/\bv\d+\b/) // a deployed profile version
  })

  test('console: connected_players responds (solo guest → none)', async () => {
    expect(await connectedPlayers(page)).toMatch(/no other players|0x/i)
  })

  // --- movement lands EXACTLY where asked (strong console round-trip) ----------
  test('console: move_player_to lands at the requested position', async () => {
    await movePlayerTo(page, 12, 1, 7)
    await expect
      .poll(
        async () => {
          const p = await position(page)
          return Math.abs(p.x - 12) < 1 && Math.abs(p.z - 7) < 1
        },
        { timeout: 15_000 }
      )
      .toBe(true)
  })

  // --- backpack: equipping a wearable bumps the deployed profile version --------
  // The clearest console-observable click: equip → setAvatar → profile re-deploy → version++.
  test('backpack: equipping a wearable bumps the profile version (get_user_data)', async () => {
    await sidebar(page, 'Backpack')
    await expectBridge(page, 'page', 'wearables')
    const before = await profileVersion(page)
    const card = page.locator('button[class*="card"]').first()
    await card.scrollIntoViewIfNeeded()
    await card.hover()
    await card.getByText(/EQUIP|UNEQUIP/i).click()
    await expectBridge(page, 'scene', 'equip')
    await expect.poll(() => profileVersion(page), { timeout: 60_000 }).toBeGreaterThan(before) // catalyst deploy can be slow
  })

  // --- backpack → Emotes tab: assigning an emote to a slot (bridge: equipEmote) --
  test('backpack: assigning an emote to a slot posts equipEmote', async () => {
    await sidebar(page, 'Backpack')
    await page.getByRole('button', { name: 'Emotes', exact: true }).click()
    await expectBridge(page, 'page', 'emotes')
    const card = page.locator('button[class*="card"]').first()
    await card.scrollIntoViewIfNeeded()
    await card.hover()
    await card.getByText(/EQUIP|UNEQUIP/i).click()
    await expectBridge(page, 'scene', 'equipEmote')
  })

  // --- emote wheel: clicking a populated slot plays that emote (bridge: triggerEmote) --
  // Runs AFTER the assign test: a fresh guest's wheel can be empty, so we click a slot the
  // previous test just filled — the aria-label carries ": <name>" only for populated slots.
  test('emotes: clicking a wheel slot plays that emote', async () => {
    await sidebar(page, 'Emotes')
    await expectBridge(page, 'page', 'emotes') // wheel data arrived
    const populated = page.getByRole('button', { name: /^Emote slot \d+: / }).first()
    await populated.click({ timeout: 30_000 })
    await expectBridge(page, 'scene', 'triggerEmote')
  })

  // --- settings: nudging a slider posts setSetting -----------------------------
  test('settings: changing a control posts setSetting', async () => {
    await sidebar(page, 'Settings')
    await expectBridge(page, 'page', 'settings')
    // Slider primitives expose an aria-labelled "increase" arrow — an unambiguous, real click.
    await page.getByRole('button', { name: 'increase' }).first().click()
    await expectBridge(page, 'scene', 'setSetting')
  })
})

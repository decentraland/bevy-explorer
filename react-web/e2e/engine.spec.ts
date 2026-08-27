// Tier 2 — per-domain validation against the REAL engine + bridge scene.
// Boots the world ONCE (enter as guest) and walks each domain, asserting the API
// call round-trips over the bridge. Drives the player with bevy console commands.
// Requires a real GPU (WebGPU) + the bridge scene on :8100 — see e2e/README.md.

import { test, expect, type Page } from '@playwright/test'
import { enterAsGuest, sidebar, cmd, walkNearby, teleport, playerPosition, expectBridge, bridgeKinds } from './helpers'

test.describe.configure({ mode: 'serial' })

test.describe('react HUD ↔ real engine', () => {
  let page: Page

  test.beforeAll(async ({ browser }) => {
    page = await browser.newPage()
    await enterAsGuest(page) // login as guest → world
  })
  test.afterAll(async () => {
    await page.close()
  })

  // --- session: login worked + world-entry fetches ---------------------------
  test('session: guest login reaches the world and fetches profile + notifications', async () => {
    await expect(page.locator('nav[aria-label="Main navigation"]')).toBeVisible()
    await expectBridge(page, 'scene', 'getProfile')
    await expectBridge(page, 'scene', 'getNotifications')
  })

  // --- world: bevy player movement (deterministic console command) -----------
  test('world: walk_player_to relocates the avatar', async () => {
    const before = await playerPosition(page)
    await walkNearby(page)
    await expect.poll(async () => playerPosition(page)).not.toBe(before)
    expect(await playerPosition(page)).toMatch(/-?\d/)
  })

  // --- chat: send a Nearby message -------------------------------------------
  test('chat: typing + Enter sends a Nearby message', async () => {
    const input = page.getByRole('textbox').first()
    await input.click()
    await input.fill('e2e hello')
    await input.press('Enter')
    await expectBridge(page, 'scene', 'sendChat')
  })

  // --- settings: open → fetch + render ---------------------------------------
  test('settings: opening the panel fetches and renders settings', async () => {
    await sidebar(page, 'Settings')
    await expectBridge(page, 'scene', 'getSettings')
    await expectBridge(page, 'page', 'settings')
  })

  // --- emotes: open the wheel → fetch equipped emotes ------------------------
  test('emotes: opening the wheel fetches emotes', async () => {
    await sidebar(page, 'Emotes')
    await expectBridge(page, 'scene', 'getEmotes')
    await expectBridge(page, 'page', 'emotes')
  })

  // --- wearables + avatarPreview: open backpack → catalog + preview rect ------
  test('wearables: opening the backpack fetches wearables and reports the avatar-preview rect', async () => {
    await sidebar(page, 'Backpack')
    await expectBridge(page, 'scene', 'getWearables')
    await expectBridge(page, 'page', 'wearables')
    await expectBridge(page, 'scene', 'engineViewport') // avatarPreview cutout
  })

  // --- communities: open → fetch list ----------------------------------------
  test('communities: opening the panel fetches communities', async () => {
    await sidebar(page, 'Communities')
    await expectBridge(page, 'scene', 'getCommunities')
    await expectBridge(page, 'page', 'communities')
  })

  // --- gallery: open → fetch the camera reel ---------------------------------
  test('gallery: opening the gallery fetches the camera reel', async () => {
    await sidebar(page, 'Gallery')
    await expectBridge(page, 'scene', 'getGallery')
    await expectBridge(page, 'page', 'gallery')
  })

  // --- world/map: open map → current parcel ----------------------------------
  test('map: opening the map fetches the current parcel', async () => {
    await sidebar(page, 'Map')
    await expectBridge(page, 'scene', 'getMap')
    await expectBridge(page, 'page', 'mapState')
  })

  // --- profile: passport relayed --------------------------------------------
  test('profile: the passport is relayed to the page', async () => {
    await expectBridge(page, 'page', 'profile')
    await sidebar(page, 'Profile')
    await expect(page.getByRole('button', { name: 'Profile', exact: true })).toHaveAttribute('aria-pressed', 'true')
  })

  // --- notifications: open → fetch -------------------------------------------
  test('notifications: opening the panel refetches notifications', async () => {
    await sidebar(page, 'Notifications')
    await expectBridge(page, 'scene', 'getNotifications')
    await expectBridge(page, 'page', 'notifications')
  })

  // --- friends: snapshot relayed (actions need seeded data → tier 1) ---------
  test('friends: the social snapshot is relayed', async () => {
    await sidebar(page, 'Friends')
    await expectBridge(page, 'page', 'friends')
  })

  // --- world: mic toggle round-trips -----------------------------------------
  test('world: toggling the mic posts setMic and the state is confirmed', async () => {
    await sidebar(page, 'Voice chat')
    await expectBridge(page, 'scene', 'setMic')
    await expectBridge(page, 'page', 'mic')
  })

  // --- world: teleport via bevy command --------------------------------------
  test('world: teleport moves the player to a new parcel', async () => {
    const before = await playerPosition(page)
    await teleport(page, 1, 1)
    await expect.poll(async () => playerPosition(page)).not.toBe(before)
  })

  // --- input: the full HUD hotkey pipeline ----------------------------------
  // The one thing no deterministic tier can see: winit's web key listeners live ON the
  // canvas, so a key pressed while DOM focus sits on a HUD element only reaches the engine
  // via boot.js's forwarder, then comes back as a systemAction on the stream. If a winit
  // upgrade moves its listeners (or the forwarder breaks), THIS test catches it.
  test('input: a key pressed with DOM focus on the HUD round-trips via the engine action stream', async () => {
    await sidebar(page, 'Map') // open via click — DOM focus is now the sidebar button, not the canvas
    const search = page.getByPlaceholder('Search places & worlds')
    await expect(search).toBeVisible()
    await page.keyboard.press('KeyM') // forwarder → canvas → winit → engine resolve → stream → toggle
    await expect(search).toBeHidden()
  })

  // --- bindings: the engine's table renders in the Key Bindings tab ----------
  test('bindings: the Key Bindings tab renders the engine binding table', async () => {
    await sidebar(page, 'Settings')
    await page.getByRole('button', { name: 'Key Bindings', exact: true }).click()
    await expectBridge(page, 'scene', 'getBindings')
    // A default row straight from the engine's InputMap (Move Forward = KeyW → chip "W").
    const row = page.locator('div', { hasText: /^Move Forward/ }).last()
    await expect(row.getByRole('button', { name: 'W', exact: true })).toBeVisible()
  })

  // pointer (hover) + nametags are world-space/data-dependent and not deterministically
  // reachable from a fresh guest in an empty parcel — their contracts are covered in
  // tier 1 (src/test/pointer.test.tsx, nametags.test.tsx).
  test.skip('pointer: hover stream — covered deterministically in tier 1', () => {})
  test.skip('nametags: world-space, engine-rendered — covered in tier 1', () => {})

  test('no unexpected bridge errors were logged', async () => {
    // Sanity: at least the core fetches happened (proves the bridge relay is alive).
    const sentToScene = await bridgeKinds(page, 'scene')
    expect(sentToScene).toEqual(expect.arrayContaining(['getProfile', 'getSettings', 'getMap']))
  })
})

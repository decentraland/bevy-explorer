// Tier 2 — REAL-engine e2e helpers (Playwright), mirroring dcl-editor's validate.mjs:
// drive the live app, the same-document engine via bevy console commands, and observe the bridge
// over a BroadcastChannel spy. Runs HEADED with a real GPU (WebGPU) — see e2e/README.
//
// Why a second tier: tier 1 (vitest) proves every API call's contract deterministically;
// this proves the guest-reachable calls actually round-trip through the real engine +
// bridge scene. Many calls (friend accept, community leave, mark-read) need seeded data
// a fresh guest doesn't have, so they live only in tier 1.

import { type Page, expect, test } from '@playwright/test'

export const APP_URL = process.env.E2E_URL ?? 'http://localhost:5173/'
export const BRIDGE_CHANNEL = 'bevy-ui-bridge'

/** Wait until the engine console RPC is live (same document as the app — no iframe). */
async function engineReady(page: Page): Promise<void> {
  for (let i = 0; i < 240; i++) {
    const ready = await page
      .evaluate(() => typeof (window as unknown as { engine_console_command?: unknown }).engine_console_command === 'function')
      .catch(() => false)
    if (ready) return
    await page.waitForTimeout(500)
  }
  throw new Error('engine console RPC never became ready')
}

/** Run a bevy/engine console command and return its string reply. */
export async function cmd(page: Page, line: string): Promise<string> {
  await engineReady(page)
  return page.evaluate(
    (l) => (window as unknown as { engine_console_command: (s: string) => Promise<string> }).engine_console_command(l),
    line
  )
}

// --- bevy world driving (deterministic — prefer these over synthetic input) ------
/** Walk the avatar to a world position via the movement controller (`move_player_to` is
 *  preview-only). Resolves on arrival (0.5 m stop threshold) or rejects after `timeout` s. */
export const walkPlayerTo = (page: Page, x: number, y: number, z: number, timeout = 20): Promise<string> =>
  cmd(page, `walk_player_to ${x} ${y} ${z} ${timeout}`)
/** Walk a short hop from where the avatar stands, trying each cardinal direction until one
 *  arrives — the walk respects colliders and the spawn has steps/props nearby, so any fixed
 *  target can be blocked. Returns the target reached. */
export async function walkNearby(page: Page, dist = 3): Promise<{ x: number; y: number; z: number }> {
  const attempt = async (): Promise<{ x: number; y: number; z: number }> => {
    const from = await position(page)
    const errors: string[] = []
    for (const [dx, dz] of [
      [dist, 0],
      [-dist, 0],
      [0, dist],
      [0, -dist]
    ]) {
      const to = { x: from.x + dx, y: from.y, z: from.z + dz }
      try {
        await walkPlayerTo(page, to.x, to.y, to.z, 10)
        return to
      } catch (e) {
        errors.push(`(${to.x}, ${to.z}): ${e instanceof Error ? e.message : String(e)}`)
      }
    }
    throw new Error(`walk_player_to blocked in every direction from (${from.x}, ${from.z}): ${errors.join('; ')}`)
  }
  try {
    return await attempt()
  } catch (first) {
    // Some Genesis Plaza spawn points sit in fenced pockets where every cardinal walk hits a
    // railing — and the plaza scene owns all its parcels, so an in-scene teleport respawns at
    // the same points. Teleport OFF the scene instead: 0,-11 is the DAO road parcel just south
    // of the plaza (flat, permanently unclaimable), then retry once from there.
    await teleport(page, 0, -11)
    await page.waitForTimeout(2000)
    try {
      return await attempt()
    } catch {
      throw first
    }
  }
}
export const teleport = (page: Page, x: number, y: number): Promise<string> => cmd(page, `teleport ${x} ${y}`)
export const playerPosition = (page: Page): Promise<string> => cmd(page, 'player_position')

// --- bevy state QUERIES — verify a click actually changed engine state ------------
/** `/player_position` parsed to numbers (`(x, y, z)`), so a teleport/move can be asserted exactly. */
export async function position(page: Page): Promise<{ x: number; y: number; z: number }> {
  const raw = await playerPosition(page)
  const m = raw.match(/\(?\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)/)
  if (!m) throw new Error(`unparseable player_position: ${raw}`)
  return { x: Number(m[1]), y: Number(m[2]), z: Number(m[3]) }
}
/** `/get_user_data` → "name (0x…): vN, web3=…". The profile VERSION bumps on every avatar change,
 *  so equipping a wearable/emote is observable here without any DOM assertion. */
export const getUserData = (page: Page): Promise<string> => cmd(page, 'get_user_data')
export async function profileVersion(page: Page): Promise<number> {
  const m = (await getUserData(page)).match(/\bv(\d+)\b/)
  return m ? Number(m[1]) : -1
}
/** `/connected_players` → comma-separated addresses, or "no other players connected". */
export const connectedPlayers = (page: Page): Promise<string> => cmd(page, 'connected_players')

// --- bridge spy: record every envelope on the bridge channel, both directions ----
export async function installBridgeSpy(page: Page): Promise<void> {
  await page.addInitScript((channel) => {
    const w = window as unknown as { __bridgeLog?: unknown[] }
    if (w.__bridgeLog) return
    w.__bridgeLog = []
    try {
      // Same lazy per-boot suffix as the app's bridgeChannelName() — the init script runs first,
      // so it seeds __bridgeSession and the app's `??=` picks it up.
      const ws = window as unknown as { __bridgeSession?: string }
      ws.__bridgeSession ??= crypto.randomUUID().slice(0, 8)
      const ch = new BroadcastChannel(`${channel}#${ws.__bridgeSession}`)
      ch.onmessage = (e: MessageEvent) => w.__bridgeLog!.push(e.data)
    } catch {
      /* BroadcastChannel unavailable — spy disabled */
    }
  }, BRIDGE_CHANNEL)
}

type Dir = 'scene' | 'page'
/** The `kind`s seen for one direction (`scene` = page→scene API calls, `page` = responses). */
export async function bridgeKinds(page: Page, to: Dir): Promise<string[]> {
  return page.evaluate((dir) => {
    const log = (window as unknown as { __bridgeLog?: { to?: string; msg?: { kind?: string } }[] }).__bridgeLog ?? []
    return log.filter((e) => e?.to === dir).map((e) => e?.msg?.kind ?? '')
  }, to)
}

/** Wait until an envelope of (direction, kind) has crossed the bridge. */
export async function expectBridge(page: Page, to: Dir, kind: string, timeout = 20000): Promise<void> {
  await expect
    .poll(async () => (await bridgeKinds(page, to)).includes(kind), { timeout, message: `bridge ${to}:${kind}` })
    .toBe(true)
}

/** Enter the world as a guest (the e2e "login" step), with the bridge spy armed. */
export async function enterAsGuest(page: Page): Promise<void> {
  // Fresh browser profile every run = cold asset cache: a Genesis Plaza boot can take 3+
  // minutes on a slow line, which is over the default 180s hook budget. Extend it for the
  // boot only; the per-test timeout is untouched.
  test.setTimeout(420_000)
  await installBridgeSpy(page)
  await page.goto(APP_URL)
  await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).click({ timeout: 90000 })
  // Entry now goes through the destination picker — skip to home (default spawn → Genesis Plaza on a fresh profile).
  await page.getByRole('button', { name: /SKIP TO HOME/i }).click({ timeout: 60000 })
  // World-ready: the React sidebar nav mounts once phase === 'world'.
  await page.waitForSelector('nav[aria-label="Main navigation"]', { timeout: 360000 })
}

/** Click a sidebar nav icon by its aria-label (Profile, Map, Settings, Emotes, …).
 *  A prior test may have left a full-screen menu page (Settings/Backpack/…) covering the
 *  sidebar — close it first (MainMenuShell's X), or the click starves behind it. */
export async function sidebar(page: Page, label: string): Promise<void> {
  for (const name of ['Close', 'Close emotes']) {
    const close = page.getByRole('button', { name, exact: true }).first()
    if (await close.isVisible().catch(() => false)) await close.click()
  }
  await page.getByRole('button', { name: label, exact: true }).click()
}

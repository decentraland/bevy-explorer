// Tier 2 — REAL-engine e2e helpers (Playwright), mirroring dcl-editor's validate.mjs:
// drive the live app, the engine via bevy console commands, and observe the bridge
// over a BroadcastChannel spy. Runs HEADED with a real GPU (WebGPU) — see e2e/README.
//
// Why a second tier: tier 1 (vitest) proves every API call's contract deterministically;
// this proves the guest-reachable calls actually round-trip through the real engine +
// bridge scene. Many calls (friend accept, community leave, mark-read) need seeded data
// a fresh guest doesn't have, so they live only in tier 1.

import { type Page, type Frame, expect } from '@playwright/test'

export const APP_URL = process.env.E2E_URL ?? 'http://localhost:5173/'
export const BRIDGE_CHANNEL = 'bevy-ui-bridge'

/** The same-origin engine iframe (served under /engine/), once its console RPC is live. */
export async function engineFrame(page: Page): Promise<Frame> {
  for (let i = 0; i < 240; i++) {
    const f = page.frames().find((fr) => fr.url().includes('/engine'))
    if (f) {
      const ready = await f
        .evaluate(() => typeof (window as unknown as { engine_console_command?: unknown }).engine_console_command === 'function')
        .catch(() => false)
      if (ready) return f
    }
    await page.waitForTimeout(500)
  }
  throw new Error('engine console RPC never became ready')
}

/** Run a bevy/engine console command and return its string reply. */
export async function cmd(page: Page, line: string): Promise<string> {
  const f = await engineFrame(page)
  return f.evaluate(
    (l) => (window as unknown as { engine_console_command: (s: string) => Promise<string> }).engine_console_command(l),
    line
  )
}

// --- bevy world driving (deterministic — prefer these over synthetic input) ------
export const movePlayerTo = (page: Page, x: number, y: number, z: number): Promise<string> =>
  cmd(page, `move_player_to ${x} ${y} ${z}`)
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
      const ch = new BroadcastChannel(channel)
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
  await installBridgeSpy(page)
  await page.goto(APP_URL)
  await page.getByRole('button', { name: /EXPLORE AS GUEST/i }).click({ timeout: 90000 })
  // World-ready: the React sidebar nav mounts once phase === 'world'.
  await page.waitForSelector('nav[aria-label="Main navigation"]', { timeout: 180000 })
}

/** Click a sidebar nav icon by its aria-label (Profile, Map, Settings, Emotes, …). */
export const sidebar = (page: Page, label: string): Promise<void> =>
  page.getByRole('button', { name: label, exact: true }).click()

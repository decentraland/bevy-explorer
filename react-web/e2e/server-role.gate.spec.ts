// The in-tab server gate: an authoritative project previewed in the editor runs its server in the
// REAL engine, switched on by the page's editor host for that preview alone. The editor's starter is
// authoritative: its server copy runs the server branch (isServer() true), a client message is
// applied by it on Play, Stop restarts it with the player's copy, and leaving turns it off. What the
// server stores outlives Stop and a page reload, and the player's copy cannot read it. The same
// project made non-authoritative gets no server copy. The editor shows the copy's console and storage.
// Run: see playwright.gate.config.ts.

import { appendFileSync } from 'node:fs'
import { expect, test } from '@playwright/test'
import { SERVERS, ENTRY, HOME, HOME_REALM, NAV, SCENES, UI, keepOffProduction } from './gate'

const PROJECT_NAME = 'Server gate'

// replaces the starter's code: the server owns a counter the client asks it to bump once it runs,
// and keeps how many bumps it has ever applied in its storage
const SCENE = `import { engine, Schemas } from '@dcl/sdk/ecs'
import { isServer, registerMessages, syncEntity } from '@dcl/sdk/network'
import { Storage } from '@dcl/sdk/server'
import { getRealm } from '~system/Runtime'
import { signedFetch } from '~system/SignedFetch'

const Counter = engine.defineComponent('srvgate:Counter', { value: Schemas.Number })
const room = registerMessages({
  bump: Schemas.Map({ by: Schemas.Number }),
  bumped: Schemas.Map({ value: Schemas.Number })
})

export function main() {
  if (isServer()) {
    console.log('SRVGATE server isServer()=true')
    const counter = engine.addEntity()
    Counter.create(counter, { value: 0 })
    syncEntity(counter, [Counter.componentId], 1)
    room.onMessage('bump', async (data, context) => {
      const value = Counter.get(counter).value + data.by
      Counter.getMutable(counter).value = value
      const stored = (((await Storage.get('bumps')) as number | null) ?? 0) + 1
      await Storage.set('bumps', stored)
      console.log('SRVGATE server applied bump from=' + context?.from + ' value=' + value + ' stored=' + stored)
      void room.send('bumped', { value })
    })
  } else {
    console.log('SRVGATE client isServer()=false')
    room.onMessage('bumped', async (data) => {
      console.log('SRVGATE client heard bumped value=' + data.value)
      // the server's storage, asked for by the player's copy directly
      const { realmInfo } = await getRealm({})
      const res = await signedFetch({ url: realmInfo!.baseUrl + '/values/bumps' })
      console.log('SRVGATE client storage read status=' + res.status)
    })
    // a second of running: only after Play, as the editor pauses the scene at its third tick
    let running = 0
    engine.addSystem((dt) => {
      if (running > 1) return
      running += dt
      if (running > 1) void room.send('bump', { by: 1 })
    })
  }
}
`

type GateWindow = Window & {
  __dclEditorHost?: { engineConsole(line: string): Promise<string> }
  __dclEditor?: unknown
  engine_console_command?: (line: string) => Promise<string>
}

test('an authoritative preview runs its server in the tab, only while the editor previews it', async ({ page }, testInfo) => {
  expect(process.env.WEB_EDITOR_DIR, 'WEB_EDITOR_DIR').toBeTruthy()
  const blocked: string[] = []
  await keepOffProduction(page.context(), blocked)
  const consoleLog = testInfo.outputPath('console.log')
  const lines: { t: number; text: string }[] = []
  page.on('console', (message) => {
    const text = message.text()
    lines.push({ t: Date.now(), text })
    appendFileSync(consoleLog, `${new Date().toISOString()} [${message.type()}] ${text}\n`)
  })
  const since = (at: number, re: RegExp): string[] => lines.filter((l) => l.t >= at && re.test(l.text)).map((l) => l.text)
  const note = (text: string): void => console.log(`[gate] ${text}`)
  page.setDefaultTimeout(60_000)

  const ui = page.locator(UI)
  const scenes = page.locator(SCENES)
  const nav = page.locator(NAV)
  const sceneTab = ui.locator('.eui-left-tabs').getByText('Scene', { exact: true })
  const card = scenes.locator('.eui-create-card', { hasText: PROJECT_NAME })
  const run = ui.locator('[data-tip^="Run the scene"]')
  const frozen = (): Promise<boolean> =>
    page
      .evaluate(() => (window as GateWindow).__dclEditorHost!.engineConsole('scene_stats'))
      .then(
        (stats) => stats.includes('frozen'),
        () => false
      )
  const homeRealm = `${testInfo.project.use.baseURL!}${HOME_REALM}`
  const starts = /starting the local server copy of/
  const stops = /stopping the local server copy of/

  // the starter (scene.json authoritativeMultiplayer: true) gets a server copy
  await page.goto(`${ENTRY}?guest=1&realm=${encodeURIComponent(homeRealm)}&position=0,0&${SERVERS}`, { waitUntil: 'commit' })
  await expect
    .poll(() => since(0, /GATE_HOME up/).length, {
      timeout: 420_000,
      message: 'the home scene runs'
    })
    .toBeGreaterThan(0)
  await nav.getByRole('button', { name: 'Create' }).click()
  await page.waitForFunction(() => (window as GateWindow).__dclEditorHost != null && (window as GateWindow).__dclEditor != null, null, { timeout: 120_000 })
  await scenes.getByRole('button', { name: '+ New scene' }).click()
  await scenes.getByText('Example', { exact: true }).click()
  await scenes.locator('.eui-home-modal input').fill(PROJECT_NAME)
  const starterAt = Date.now()
  await scenes.getByRole('button', { name: 'Create scene' }).click()
  await expect
    .poll(() => since(starterAt, /spawning scene "b64-/).length, {
      timeout: 180_000,
      message: 'the starter runs'
    })
    .toBeGreaterThan(0)
  await ui.locator('.eui-toolbar').waitFor({ timeout: 120_000 })
  await expect.poll(frozen, { timeout: 60_000, message: 'the editor paused it' }).toBe(true)
  await expect
    .poll(() => since(starterAt, starts).length, {
      timeout: 60_000,
      message: 'the starter gets a server copy'
    })
    .toBe(1)
  note(`starter previewed: ${JSON.stringify(since(starterAt, /local s(erver|cene server)/))}`)

  // give it server code while it is closed, then open it again: the editor builds it from its files
  const backAt = Date.now()
  await ui.locator('.eui-topbar-home').click()
  await card.waitFor({ timeout: 60_000 })
  await expect
    .poll(() => since(backAt, /GATE_HOME up/).length, {
      timeout: 120_000,
      message: 'back home'
    })
    .toBeGreaterThan(0)
  await expect
    .poll(() => since(backAt, /local scene server off/).length, {
      message: 'off once home'
    })
    .toBeGreaterThan(0)
  const writeProject = (source: string | null, authoritative: boolean): Promise<string> =>
    page.evaluate(
      async ({ name, source, authoritative }) => {
        const root = await (await navigator.storage.getDirectory()).getDirectoryHandle('dcl-editor')
        const index = await (await root.getFileHandle('index.json')).getFile()
        const { projects } = JSON.parse(await index.text()) as {
          projects: Array<{ id: string; name: string }>
        }
        const id = projects.find((project) => project.name === name)!.id
        await navigator.locks.request(`dcl-editor-project:${id}`, async () => {
          const dir = await (await root.getDirectoryHandle('projects')).getDirectoryHandle(id)
          const write = async (handle: FileSystemDirectoryHandle, file: string, text: string): Promise<void> => {
            const writable = await (await handle.getFileHandle(file, { create: true })).createWritable()
            await writable.write(text)
            await writable.close()
          }
          const sceneJson = JSON.parse(await (await (await dir.getFileHandle('scene.json')).getFile()).text()) as Record<string, unknown>
          await write(dir, 'scene.json', JSON.stringify({ ...sceneJson, authoritativeMultiplayer: authoritative }, null, 2))
          if (source != null) await write(await dir.getDirectoryHandle('src'), 'index.ts', source)
        })
        return id
      },
      { name: PROJECT_NAME, source, authoritative }
    )
  const projectId = await writeProject(SCENE, true)
  note(`project ${projectId} has server code`)

  const openAt = Date.now()
  await card.click()
  await ui.locator('.eui-toolbar').waitFor({ timeout: 120_000 })
  await expect
    .poll(() => since(openAt, starts).length, {
      timeout: 120_000,
      message: 'a server copy starts'
    })
    .toBe(1)
  await expect
    .poll(() => since(openAt, /SRVGATE server isServer\(\)=true/).length, {
      timeout: 60_000,
      message: 'the server branch runs'
    })
    .toBeGreaterThan(0)
  await expect.poll(frozen, { timeout: 60_000, message: 'the editor paused it' }).toBe(true)
  note(`opened: ${JSON.stringify(since(openAt, /local s(erver|cene server)|SRVGATE/))}`)

  const play = async (stored: number): Promise<number> => {
    const at = Date.now()
    await sceneTab.click()
    await run.click()
    await ui.locator('[data-tip^="Scene is running"]').waitFor()
    await expect
      .poll(() => since(at, /SRVGATE server applied bump/).length, {
        timeout: 60_000,
        message: 'the server applies the client message'
      })
      .toBe(1)
    await expect
      .poll(() => since(at, /SRVGATE client heard bumped value=1/).length, {
        timeout: 60_000,
        message: 'and the client hears it'
      })
      .toBe(1)
    expect(since(at, /SRVGATE server applied bump/)[0], 'the stored count goes on from the last run').toContain(`stored=${stored}`)
    await expect
      .poll(() => since(at, /SRVGATE client storage read status=/), { timeout: 30_000, message: 'the client asked' })
      .toEqual([expect.stringContaining('status=403')])
    note(`Play: ${JSON.stringify(since(at, /SRVGATE|local server/))}`)
    return at
  }
  await play(1)

  // the editor's logs drawer: the server copy's console, and the storage it keeps
  await ui.getByRole('button', { name: 'Show build / server logs' }).click()
  const logTab = (name: string) => ui.locator('.eui-logs-tabs button', { hasText: name })
  await logTab('Build').click()
  await expect(ui.locator('.eui-logs-body'), 'the server copy console in Build').toContainText('SRVGATE server applied bump', { timeout: 30_000 })
  await logTab('Storage').click()
  await expect(ui.locator('.eui-value-row', { hasText: 'bumps' }), 'the server copy storage in Storage').toBeVisible({ timeout: 30_000 })
  await ui.getByRole('button', { name: 'Hide logs' }).first().click()

  // Stop restarts the scene from tick 0, and its server with it: Play again counts from 0 again
  const stopAt = Date.now()
  await ui.locator('[data-tip="Restart the scene from tick 0"]').click()
  await run.waitFor({ timeout: 60_000 })
  await expect
    .poll(() => since(stopAt, stops).length, {
      timeout: 60_000,
      message: 'the old copy stops'
    })
    .toBeGreaterThan(0)
  await expect
    .poll(() => since(stopAt, starts).length, {
      timeout: 60_000,
      message: 'a new copy starts'
    })
    .toBeGreaterThan(0)
  await expect.poll(frozen, { timeout: 60_000, message: 'paused after Stop' }).toBe(true)
  await play(2)

  // a page reload straight into the project: the stored count is still there
  const reloadAt = Date.now()
  await page.goto(`${ENTRY}?guest=1&editor=${projectId}&realm=${encodeURIComponent(homeRealm)}&position=0,0&${SERVERS}`, { waitUntil: 'commit' })
  await ui.locator('.eui-toolbar').waitFor({ timeout: 420_000 })
  await expect.poll(() => since(reloadAt, starts).length, { timeout: 120_000, message: 'a server copy starts after the reload' }).toBe(1)
  await expect.poll(frozen, { timeout: 60_000, message: 'the editor paused it' }).toBe(true)
  await play(3)

  // leaving the editor turns the server off
  const exitAt = Date.now()
  await ui.locator('.eui-topbar-menu-wrap button[data-tip="Settings"]').click()
  const back = (await ui.getByRole('button', { name: 'Back to Decentraland' }).boundingBox())!
  await page.mouse.click(back.x + back.width / 2, back.y + back.height / 2)
  await nav.waitFor({ timeout: 120_000 })
  await expect
    .poll(() => since(exitAt, stops).length, {
      timeout: 60_000,
      message: 'the copy stops'
    })
    .toBeGreaterThan(0)
  await expect
    .poll(() => since(exitAt, /local scene server off/).length, {
      timeout: 60_000,
      message: 'and the switch is off'
    })
    .toBeGreaterThan(0)
  const stats = await page.evaluate(() => (window as GateWindow).engine_console_command!('/scene_render_stats'))
  note(`after leaving: ${JSON.stringify(stats)}`)
  expect(JSON.stringify(stats)).toContain('local_servers=0')
  await expect(page.locator(HOME)).toHaveCount(0)

  // the same project, not authoritative: previewed with no server copy
  await writeProject(null, false)
  const plainAt = Date.now()
  await nav.getByRole('button', { name: 'Create' }).click()
  await card.click()
  await ui.locator('.eui-toolbar').waitFor({ timeout: 120_000 })
  await expect
    .poll(() => since(plainAt, /SRVGATE client isServer\(\)=false/).length, {
      timeout: 120_000,
      message: 'the scene runs'
    })
    .toBeGreaterThan(0)
  await expect.poll(frozen, { timeout: 60_000, message: 'the editor paused it' }).toBe(true)
  expect(since(plainAt, starts), 'a scene that is not authoritative gets no server copy').toEqual([])
  expect(since(plainAt, /SRVGATE server/)).toEqual([])
  expect(blocked, 'deployments to real servers').toEqual([])
})

// The editor gate: the sidebar's Create button opens the scene editor package (a dcl-editor
// checkout's packages/web/dist, served under /editor/) inside this page; a scene is created from
// its starter, built and published in the browser, edited, played and stopped in the REAL engine,
// Exit gives the HUD back, and the menu's Create item opens it again with the scene still there.
// Nothing serves the scene but the service worker, and a guest's editor asks the project service
// for nothing. Each numbered step passes or fails on its own. A second test opens it with
// `?editor=<project>`. Run: see playwright.gate.config.ts.

import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test, type Page } from '@playwright/test'
import { HOME_REALM, NAV, PROJECTS, UI, keepOffProduction } from './gate'
import { cmd, position } from './helpers'

const PROJECT_NAME = 'Gate scene'
const PROJECT_ID = 'gate-scene'
const MARKER = /GATE_MARKER v(\d+)/
const ENTITY_NAME = 'Gate entity'
const COMPOSITE = 'assets/scene/main.composite'

interface Host {
  version: number
  pageDir: string
  busSession: string
  openProject: string | null
  services: { projects: string | null; worldsContent: string }
  identity(): { address: string | null; isGuest: boolean }
  engineConsole(line: string): Promise<string>
  setMode(mode: string): void
  openPreview(projectId: string, position: string): Promise<void>
  spawnEditorScene(): Promise<{ hash: string }>
}
interface Spy {
  openPreview: string[]
  spawned: string[]
  console: string[]
  modes: string[]
  sceneReady: (string | null)[]
  bridgeReady: number
  hello: number
  canvasKeys: number
  channels: BroadcastChannel[]
}
interface Manifest {
  version: number
  entity: { id: string; pointers: string[]; content: { file: string; hash: string }[] }
}
type GateWindow = Window & {
  __dclEditorHost?: Host
  __dclEditor?: unknown
  __gate?: Spy
  engine_console_command?: (line: string) => Promise<string>
}

// Records what the editor asks of the host, and both buses, from here on.
async function installSpies(page: Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as GateWindow
    const host = w.__dclEditorHost!
    const spy: Spy = { openPreview: [], spawned: [], console: [], modes: [], sceneReady: [], bridgeReady: 0, hello: 0, canvasKeys: 0, channels: [] }
    w.__gate = spy
    const { openPreview, spawnEditorScene, engineConsole, setMode } = host
    host.openPreview = (id, position) => {
      spy.openPreview.push(`${id}@${position}`)
      return openPreview(id, position)
    }
    host.spawnEditorScene = async () => {
      const scene = await spawnEditorScene()
      spy.spawned.push(scene.hash)
      return scene
    }
    host.engineConsole = (line) => {
      spy.console.push(line)
      return engineConsole(line)
    }
    host.setMode = (mode) => {
      spy.modes.push(mode)
      setMode(mode)
    }
    const editorBus = new BroadcastChannel(`dcl-editor-bus#${host.busSession}`)
    editorBus.onmessage = ({ data }: MessageEvent<{ to?: string; msg?: { type?: string; scene?: { hash?: string } | null } }>) => {
      if (data.to === 'page' && data.msg?.type === 'scene-ready') spy.sceneReady.push(data.msg.scene?.hash ?? null)
    }
    const bridgeBus = new BroadcastChannel(`bevy-ui-bridge#${host.busSession}`)
    bridgeBus.onmessage = ({ data }: MessageEvent<{ to?: string; msg?: { kind?: string } }>) => {
      if (data.to === 'page' && data.msg?.kind === 'bridgeReady') spy.bridgeReady++
      if (data.to === 'scene' && data.msg?.kind === 'hello') spy.hello++
    }
    spy.channels.push(editorBus, bridgeBus)
    document.getElementById('mygame-canvas')!.addEventListener('keydown', () => spy.canvasKeys++)
  })
}

const spy = (page: Page): Promise<Omit<Spy, 'channels'>> =>
  page.evaluate(() => {
    const { channels: _, ...rest } = (window as GateWindow).__gate!
    return rest
  })

const manifest = (page: Page): Promise<Manifest | null> =>
  page.evaluate(async (id) => {
    const pageDir = (window as GateWindow).__dclEditorHost!.pageDir
    const stored = await (await caches.open('dcl-editor-preview-v1')).match(`${pageDir}preview/${id}/__manifest`)
    return stored == null ? null : ((await stored.json()) as Manifest)
  }, PROJECT_ID)

// A file of the project as the editor's store keeps it (Origin Private File System).
const storedFile = (page: Page, path: string): Promise<string | null> =>
  page.evaluate(
    async ({ id, path }) => {
      try {
        const parts = ['dcl-editor', 'projects', id, ...path.split('/')]
        const name = parts.pop()!
        let dir = await navigator.storage.getDirectory()
        for (const part of parts) dir = await dir.getDirectoryHandle(part)
        return await (await (await dir.getFileHandle(name)).getFile()).text()
      } catch {
        return null
      }
    },
    { id: PROJECT_ID, path }
  )

test('the scene editor opens, edits and plays a starter scene inside the page', async ({ page }, testInfo) => {
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
  page.on('pageerror', (error) => appendFileSync(consoleLog, `${new Date().toISOString()} [pageerror] ${error.message}\n`))
  const seen = (re: RegExp, since = 0): boolean => lines.some((l) => l.t >= since && re.test(l.text))
  const sawMarker = (version: number, since = 0): boolean =>
    lines.some((l) => l.t >= since && Number(MARKER.exec(l.text)?.[1]) === version)

  const shots = process.env.GATE_SHOTS ?? testInfo.outputPath('shots')
  mkdirSync(shots, { recursive: true })
  const shot = (name: string): Promise<unknown> => page.screenshot({ path: join(shots, `${name}.png`) }).catch(() => {})

  const evidence: string[] = []
  const startedAt = Date.now()
  const note = (text: string): void => {
    const line = `+${((Date.now() - startedAt) / 1000).toFixed(1)}s ${text}`
    evidence.push(line)
    console.log(`[gate] ${line}`)
  }
  const failed: string[] = []
  // A failed step is reported and the ones after it still run.
  const step = async (name: string, body: () => Promise<void>): Promise<boolean> => {
    try {
      await body()
      note(`STEP ${name}: PASS`)
      return true
    } catch (e) {
      failed.push(name)
      note(`STEP ${name}: FAIL ${(e instanceof Error ? e.message : String(e)).split('\n').slice(0, 6).join(' | ')}`)
      await shot(`failed-${name.split(' ')[0]}`)
      return false
    }
  }

  page.setDefaultTimeout(60_000)
  const ui = page.locator(UI)
  const sceneTab = ui.locator('.eui-left-tabs').getByText('Scene', { exact: true })
  const nav = page.locator(NAV)
  const realm = (): string | null => new URL(page.url()).searchParams.get('realm')
  // the engine's own word on every scene it starts
  const spawns = (since: number): string[] =>
    lines.filter((l) => l.t >= since).flatMap((l) => /spawning scene "([^"]+)"/.exec(l.text)?.[1] ?? [])
  // false too while a reloaded scene is not pinned yet
  const frozen = (): Promise<boolean> =>
    page.evaluate(() => (window as GateWindow).__dclEditorHost!.engineConsole('scene_stats')).then(
      (stats) => stats.includes('frozen'),
      () => false
    )
  const origin = testInfo.project.use.baseURL!
  const homeRealm = `${origin}${HOME_REALM}`
  let entityId = ''
  let editorScene = ''

  // the engine only takes absolute content urls, so the editor scene is exported for one origin
  const about = await (await page.request.get('/editor/scene/about')).text()
  expect(about, `the editor scene must be exported with --editor-base ${origin}/editor/`).toContain(`baseUrl=${origin}/editor/scene/`)

  // What the page asks of the editor package and of its own host script. The dev server's
  // `?worker&url` module only names that script's url.
  const editorRequests: string[] = []
  const serviceRequests: string[] = []
  page.on('request', (request) => {
    const { pathname, search } = new URL(request.url())
    if (/^\/editor\/|\/editorHost\/host\//.test(pathname) && search !== '?worker&url') editorRequests.push(pathname)
    if (request.url().startsWith(PROJECTS)) serviceRequests.push(`${request.method()} ${pathname}`)
  })

  await step('1 the sidebar Create button loads the editor, and nothing of it before the click', async () => {
    // a first visit: the page reloads itself once its service worker is active, then boots the engine
    await page.goto(`/?guest=1&realm=${encodeURIComponent(homeRealm)}&position=0,0`, { waitUntil: 'commit' })
    await expect.poll(() => seen(/GATE_HOME up/), { timeout: 420_000, message: 'the home scene runs' }).toBe(true)
    const create = nav.getByRole('button', { name: 'Create' })
    await create.waitFor({ timeout: 120_000 })
    // a clock that is not the engine's default, to find again after Exit
    await page.evaluate(() => (window as GateWindow).engine_console_command!('/time 20 7'))
    expect(editorRequests, 'nothing of the editor is requested before the click').toEqual([])
    expect(await page.evaluate(() => (window as GateWindow).__dclEditorHost == null)).toBe(true)
    await shot('0-create-in-the-rail')
    await create.click()
    await page.waitForFunction(() => (window as GateWindow).__dclEditorHost != null && (window as GateWindow).__dclEditor != null, null, {
      timeout: 120_000
    })
    await installSpies(page)
    await ui.locator('.eui-home').waitFor({ timeout: 60_000 })
    await ui.getByText('Your scenes').waitFor()
    const info = await page.evaluate(() => {
      const { version, openProject, services, identity } = (window as GateWindow).__dclEditorHost!
      const script = document.querySelector<HTMLScriptElement>('script[src$="/editor/editor.js"]')
      return { version, openProject, services, identity: identity(), script: script?.src ?? null }
    })
    note(`editor script ${info.script}; host v${info.version}, services ${JSON.stringify(info.services)}; identity ${JSON.stringify(info.identity)}; url ${page.url()}`)
    note(`requested: ${JSON.stringify(editorRequests.slice(0, 4))}`)
    expect(info.script, 'loaded from <PAGE_DIR>editor/').toBe(`${origin}/editor/editor.js`)
    expect(info).toMatchObject({ version: 1, openProject: null, services: { projects: PROJECTS } })
    expect(info.identity.isGuest).toBe(true)
    expect(new URL(page.url()).searchParams.has('editor'), 'Create does not put the flag in the url').toBe(false)
    await expect(nav, 'the HUD chrome is hidden in edit mode').toHaveCount(0)
    await expect(page.locator('#mygame-canvas')).toHaveCount(1)
    await shot('1-editor-home')
  })

  const identityBefore = await page.evaluate(() => (window as GateWindow).__dclEditorHost?.identity().address ?? null).catch(() => null)

  const created = await step('2 a starter project is created, built and published, and the engine travels to it', async () => {
    await ui.getByRole('button', { name: '+ New scene' }).click()
    await ui.getByText('Example', { exact: true }).click()
    await ui.locator('.eui-home-modal input').fill(PROJECT_NAME)
    const createdAt = Date.now()
    await ui.getByRole('button', { name: 'Create scene' }).click()
    await expect.poll(async () => (await spy(page)).openPreview, { timeout: 180_000, message: 'host.openPreview is called' }).toEqual([`${PROJECT_ID}@0,0`])
    const published = (await manifest(page))!
    entityId = published.entity.id
    const machineId = await page.evaluate(() => localStorage.getItem('dcl-editor.machineId'))
    note(`built + published ${Date.now() - createdAt} ms after Create: entity ${entityId}, v${published.version}, ${published.entity.content.length} files`)
    expect(Buffer.from(entityId.slice(4), 'base64').toString(), 'a b64- preview id').toBe(`/preview/${PROJECT_ID}-${machineId}`)
    expect(published.entity.content.map((c) => c.file)).toEqual(expect.arrayContaining(['bin/index.js', 'scene.json']))
    expect(await storedFile(page, 'src/index.ts'), 'the project is stored in the browser').toContain('engine.addEntity()')
    await expect.poll(() => spawns(createdAt), { timeout: 60_000, message: 'the engine starts the project scene' }).toContain(entityId)
    expect(seen(new RegExp(`realm change .*/preview/${PROJECT_ID}/about`), createdAt), 'by changing realm, in the same page').toBe(true)
    expect(realm(), 'the page url never records the preview realm').toBe(homeRealm)
  })

  const attached =
    created &&
    (await step('3 the editor scene is spawned privileged, connects over the bus, and the hierarchy lists the scene', async () => {
      await expect.poll(async () => (await spy(page)).spawned.length, { timeout: 60_000, message: 'host.spawnEditorScene resolves' }).toBe(1)
      editorScene = (await spy(page)).spawned[0]
      await expect
        .poll(async () => (await spy(page)).sceneReady, { timeout: 120_000, message: 'scene-ready names the project scene' })
        .toContain(entityId)
      await ui.locator('.eui-toolbar').waitFor({ timeout: 60_000 })
      await expect(ui.locator('.eui-loading')).toHaveCount(0, { timeout: 60_000 })
      await expect.poll(frozen, { timeout: 30_000, message: 'the editor paused the scene' }).toBe(true)
      // The starter has no composite: its one entity (a cube) is made by code when the scene runs,
      // and whether it ran before the editor paused it varies. The title is the scene's either way.
      // A scene with nothing in it opens on the Prefabs tab, once its first snapshot is in.
      await expect(async () => {
        await sceneTab.click()
        await expect(ui.locator('.eui-left .eui-title')).toHaveText(PROJECT_NAME, { timeout: 1000 })
      }).toPass({ timeout: 30_000 })
      note(`editor scene ${editorScene}; scene-ready for ${JSON.stringify((await spy(page)).sceneReady)}`)
      note(`hierarchy: ${JSON.stringify((await ui.locator('.eui-left').innerText()).split('\n').filter(Boolean))}`)
      const identity = await page.evaluate(() => (window as GateWindow).__dclEditorHost!.identity().address)
      expect(identity, 'the editor scene did not log in over the player').toBe(identityBefore)
      expect(seen(/hosted — another privileged scene owns the session/), 'the editor scene saw the HUD scene and left the session alone').toBe(true)
      await expect(page.locator('[aria-label="Scene permission request"]')).toHaveCount(0)
      await shot('2-editor-docked')
    }))

  if (attached) {
    await step('4 edits are stored, rebuilt and published; a code save reloads the scene by id and it logs the new marker', async () => {
      // an entity made in the hierarchy: autosaved as the composite, which the rebuild publishes
      const row = ui.locator('.eui-left .eui-row', { hasText: ENTITY_NAME })
      const start = (await manifest(page))!.version
      await sceneTab.click()
      await ui.locator('[data-tip="New entity"]').click()
      await ui.getByPlaceholder('Entity name').fill(ENTITY_NAME)
      await ui.getByRole('button', { name: 'Create', exact: true }).click()
      await expect(row).toHaveCount(1)
      await expect.poll(() => storedFile(page, COMPOSITE), { timeout: 30_000, message: 'the composite is in the store' }).toContain(ENTITY_NAME)
      await expect
        .poll(async () => (await manifest(page))!.entity.content.map((c) => c.file), { timeout: 30_000, message: 'the composite is published' })
        .toEqual(expect.arrayContaining([COMPOSITE, 'main.crdt']))
      note(`entity "${ENTITY_NAME}" autosaved to ${COMPOSITE}; manifest v${start} -> v${(await manifest(page))!.version}`)

      const save = async (version: number, replace: boolean): Promise<number> => {
        await page.keyboard.press('ControlOrMeta+Home')
        if (replace) await page.keyboard.press('Shift+ArrowDown')
        await page.keyboard.type(`console.log('GATE_MARKER v${version}')\n`)
        const at = Date.now()
        await page.keyboard.press('ControlOrMeta+s')
        await expect.poll(() => sawMarker(version, at), { timeout: 60_000, message: `the scene logs v${version}` }).toBe(true)
        const ms = lines.find((l) => l.t >= at && MARKER.test(l.text))!.t - at
        // the save restarts the scene; the editor pauses the new one a moment later
        await expect.poll(frozen, { timeout: 30_000, message: 'paused again after the save' }).toBe(true)
        return ms
      }
      const before = (await manifest(page))!.version
      const { canvasKeys: keysBefore, console: sent } = await spy(page)
      const consoleBefore = sent.length
      await ui.getByRole('button', { name: 'Code', exact: true }).click()
      await ui.locator('.cm-content').click()
      const first = await save(1, false)
      const second = await save(2, true)
      await ui.locator('.eui-studio-filehead', { hasText: 'Saved' }).waitFor()
      await shot('3-code-saved')
      const after = (await manifest(page))!
      const transport = (await spy(page)).console.slice(consoleBefore).filter((line) => /^(reload|freeze_scene|unfreeze_scene)\b/.test(line))
      note(`save -> marker: v1 ${first} ms, v2 ${second} ms; manifest v${before} -> v${after.version}; console ${JSON.stringify(transport)}`)
      expect(after.entity.id, 'the entity id is stable').toBe(entityId)
      expect(after.version).toBeGreaterThanOrEqual(before + 2)
      expect(transport.filter((line) => line === `reload ${entityId}`).length, 'reloaded by id').toBeGreaterThanOrEqual(2)
      expect(await storedFile(page, 'src/index.ts'), 'the edit is in the store').toMatch(/^console\.log\('GATE_MARKER v2'\)\n/)
      expect((await spy(page)).canvasKeys, 'what is typed in the editor stays out of the engine').toBe(keysBefore)
      await ui.locator('.eui-studio-hbtn').click()
      await ui.locator('.eui-toolbar').waitFor()
      await expect(row, 'the entity came back from the published build').toHaveCount(1)
    })

    await step('5 Play then Stop restarts only the project scene and returns the player to its spawn', async () => {
      const before = await spy(page)
      const playAt = Date.now()
      const run = ui.locator('[data-tip^="Run the scene"]')
      expect(await frozen(), 'paused before Play').toBe(true)
      await sceneTab.click()
      await run.click()
      await expect.poll(async () => (await spy(page)).modes.at(-1), { timeout: 60_000, message: 'the host is in play mode' }).toBe('play')
      await ui.locator('[data-tip^="Scene is running"]').waitFor()
      await expect.poll(frozen, { timeout: 30_000, message: 'the scene runs' }).toBe(false)
      // running, the starter's code makes its cube
      await expect(ui.locator('.eui-left .eui-row', { hasText: 'Box' })).toHaveCount(1, { timeout: 30_000 })
      await shot('4-playing')
      // the player walks off while playing: Stop puts them back at the scene's spawn, the parcel's centre
      await cmd(page, 'walk_player_to 3 0 3 10')
      const walked = await position(page)
      expect(Math.hypot(walked.x - 8, walked.z - 8), 'the player walked away from the spawn').toBeGreaterThan(4)
      await ui.locator('[data-tip="Restart the scene from tick 0"]').click()
      await expect.poll(async () => (await spy(page)).modes.at(-1), { timeout: 60_000, message: 'back in edit mode' }).toBe('edit')
      await run.waitFor({ timeout: 60_000 })
      await expect.poll(frozen, { timeout: 30_000, message: 'paused again after Stop' }).toBe(true)
      const after = await spy(page)
      const fromSpawn = async (): Promise<number> => {
        const at = await position(page)
        return Math.hypot(at.x - 8, at.z - 8)
      }
      await expect.poll(fromSpawn, { timeout: 15_000, message: 'Stop returns the player to the spawn' }).toBeLessThan(1)
      note(`player walked to (${walked.x.toFixed(1)}, ${walked.z.toFixed(1)}) while playing; after Stop: ${await cmd(page, 'player_position')}`)
      const reloads = after.console.filter((line) => line.startsWith('reload'))
      note(`engine spawned ${JSON.stringify(spawns(playAt))}; bridgeReady ${before.bridgeReady} -> ${after.bridgeReady}; hello ${before.hello} -> ${after.hello}`)
      note(`reloads ${JSON.stringify(reloads)}`)
      // Stop starts the project scene again, and nothing else: not the bridge scene, not the editor's
      expect(spawns(playAt)).toEqual([entityId])
      expect(after.bridgeReady, 'the bridge scene did not start again').toBe(before.bridgeReady)
      expect(after.hello).toBe(before.hello)
      expect(reloads.every((line) => line === `reload ${entityId}`), 'every reload names the project scene').toBe(true)
      await expect(nav, 'still no HUD chrome').toHaveCount(0)
      // its wearables are a catalyst's: the preview realm forwards those pointers
      expect(seen(/failed to resolve body/), 'the avatar has a body on the preview realm').toBe(false)
    })
  }

  await step('6 Exit gives the HUD back at the realm the player came from', async () => {
    if (await ui.locator('.eui-topbar-home').count()) await ui.locator('.eui-topbar-home').click()
    const exitAt = Date.now()
    // by position: the button is gone before a locator click has finished with it
    const back = (await ui.getByRole('button', { name: 'Back to Decentraland' }).boundingBox())!
    await page.mouse.click(back.x + back.width / 2, back.y + back.height / 2)
    await nav.waitFor({ timeout: 120_000 })
    await expect.poll(() => seen(/GATE_HOME up/, exitAt), { timeout: 120_000, message: 'the home scene runs again' }).toBe(true)
    await expect(page.locator('#dcl-editor-host > *')).toHaveCount(0)
    await page.waitForTimeout(1500)
    const clock = await page.evaluate(() => (window as GateWindow).__dclEditorHost!.engineConsole('time'))
    note(`url after exit: ${page.url()}; clock: ${clock}`)
    expect(realm()).toBe(homeRealm)
    expect(clock, 'the editor stopped the clock at noon to edit: the player has their own back').toMatch(/-> 20:\d+, speed 7 /)
    await shot('5-back-in-the-hud')
  })

  await step('7 the menu top bar Create item opens the editor again, without loading it again, and the scene made before opens', async () => {
    await nav.getByRole('button', { name: 'Settings', exact: true }).click()
    const item = page.locator('header button[data-page="create"]')
    await item.waitFor()
    // the page fades in
    await page.waitForTimeout(600)
    await shot('6-create-in-the-top-bar')
    const loads = editorRequests.filter((path) => path === '/editor/editor.js').length
    await item.click()
    await ui.locator('.eui-home').waitFor({ timeout: 60_000 })
    await expect(nav, 'the HUD chrome is hidden again').toHaveCount(0)
    await expect(page.locator('header button[data-page="settings"]'), 'the menu page closed').toHaveCount(0)
    expect(editorRequests.filter((path) => path === '/editor/editor.js').length, 'mounted again, not fetched again').toBe(loads)
    const card = ui.locator('.eui-scene-card', { hasText: PROJECT_NAME })
    await expect(card, 'the home screen lists the scene made before Exit').toHaveCount(1)
    await shot('7-editor-home-again')
    const reopenedAt = Date.now()
    await card.click()
    await expect.poll(() => spawns(reopenedAt), { timeout: 120_000, message: 'the engine runs the scene again' }).toContain(entityId)
    await ui.locator('.eui-toolbar').waitFor({ timeout: 120_000 })
    await expect.poll(frozen, { timeout: 60_000, message: 'the editor attached to it again' }).toBe(true)
    await expect(async () => {
      await sceneTab.click()
      await expect(ui.locator('.eui-left .eui-title')).toHaveText(PROJECT_NAME, { timeout: 1000 })
    }).toPass({ timeout: 30_000 })
    note(`after re-entry the scene opened again: engine spawned ${JSON.stringify(spawns(reopenedAt))}, editor scenes ${JSON.stringify((await spy(page)).spawned)}`)
    await shot('8-scene-open-again')
    await ui.locator('.eui-topbar-home').click()
    const back = (await ui.getByRole('button', { name: 'Back to Decentraland' }).boundingBox())!
    await page.mouse.click(back.x + back.width / 2, back.y + back.height / 2)
    await nav.waitFor({ timeout: 120_000 })
  })

  note(`requests to the project service as a guest: ${JSON.stringify(serviceRequests)}`)
  await testInfo.attach('gate-evidence', { body: evidence.join('\n'), contentType: 'text/plain' })
  expect(failed, 'failed steps').toEqual([])
  expect(serviceRequests, 'a guest sends the project service nothing').toEqual([])
  expect(blocked, 'deployments to real servers').toEqual([])
})

test('?editor=<project> opens the editor once in-world and hands it the project', async ({ page }, testInfo) => {
  const lines: string[] = []
  page.on('console', (message) => lines.push(message.text()))
  const homeRealm = `${testInfo.project.use.baseURL!}${HOME_REALM}`
  await page.goto(`/?guest=1&editor=${PROJECT_ID}&realm=${encodeURIComponent(homeRealm)}&position=0,0`, { waitUntil: 'commit' })
  await expect.poll(() => lines.some((line) => /GATE_HOME up/.test(line)), { timeout: 420_000, message: 'the home scene runs' }).toBe(true)
  await page.locator(UI).locator('.eui-home').waitFor({ timeout: 120_000 })
  expect(await page.evaluate(() => (window as GateWindow).__dclEditorHost!.openProject)).toBe(PROJECT_ID)
  await expect(page.locator(NAV), 'the HUD chrome is hidden in edit mode').toHaveCount(0)
  await expect.poll(() => new URL(page.url()).searchParams.get('editor'), { message: "the engine's url sync keeps the flag" }).toBe(PROJECT_ID)
})

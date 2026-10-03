// The editor gate: what this page owes the scene editor package (a dcl-editor checkout's
// packages/web/dist, served under /editor/), in the REAL engine. Create loads nothing until it is
// clicked; a new scene is previewed from the service worker under a random id; the editor's scene
// spawns privileged next to the HUD's; a code save reloads only that scene, by id; Play and Stop
// never restart the HUD scene; leaving gives the HUD, realm and clock back; `?editor=<project>`
// opens the scene directly. The editor's own UI is tested in its repo. Run: see playwright.gate.config.ts.

import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test, type Page } from '@playwright/test'
import { SERVERS, ENTRY, HOME, HOME_REALM, NAV, PROJECTS, SCENES, UI, editorBase, keepOffProduction } from './gate'

const PROJECT_NAME = 'Gate scene'
// a new scene's id carries a random tail: step 2 reads it back from the store
let PROJECT_ID = 'gate-scene'
const MARKER = /GATE_MARKER v(\d+)/

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

// under the random id the project is previewed at, which the spies saw
const manifest = (page: Page): Promise<Manifest | null> =>
  page.evaluate(async () => {
    const w = window as GateWindow
    const id = w.__gate!.openPreview.at(-1)?.split('@')[0]
    const stored = id == null ? undefined : await (await caches.open('dcl-editor-preview-v1')).match(`${w.__dclEditorHost!.pageDir}preview/${id}/__manifest`)
    return stored == null ? null : ((await stored.json()) as Manifest)
  })

test('the page hosts the scene editor: Create, preview, privileged scene, reload by id, Play, leaving', async ({ page }, testInfo) => {
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
  const editorJs = `${editorBase(origin)}editor.js`
  let entityId = ''
  let editorScene = ''

  // What the page asks of the editor package and of its own Create chunk (all of features/editorHost
  // but the HUD's config and mode). The dev server's `?worker&url` module only names a script's url.
  const editorRequests: string[] = []
  const serviceRequests: string[] = []
  page.on('request', (request) => {
    const { pathname, search } = new URL(request.url())
    if (request.url().startsWith(editorBase(origin))) editorRequests.push(request.url().split('?')[0])
    else if (/\/editorHost\/(?!config\.ts|hudMode\.ts)|\/EditorHost-[\w-]+\.js$/.test(pathname) && search !== '?worker&url') editorRequests.push(pathname)
    if (request.url().startsWith(PROJECTS)) serviceRequests.push(`${request.method()} ${pathname}`)
  })

  const scenes = page.locator(SCENES)
  const createPage = page.locator(HOME)
  let preview = ''

  await step('1 the sidebar Create button opens the Create page, the editor lists the scenes there, and nothing of it loads before the click', async () => {
    // a first visit: the page reloads itself once its service worker is active, then boots the engine
    await page.goto(`${ENTRY}?guest=1&realm=${encodeURIComponent(homeRealm)}&position=0,0&${SERVERS}`, { waitUntil: 'commit' })
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
    await scenes.locator('.eui-create').getByText('Your scenes').waitFor({ timeout: 60_000 })
    await expect(page.locator('header button[data-page="create"][aria-current="page"]'), 'Create is the menu page open').toHaveCount(1)
    const info = await page.evaluate(() => {
      const { version, openProject, services, identity } = (window as GateWindow).__dclEditorHost!
      const script = document.querySelector<HTMLScriptElement>('script[src$="/editor.js"]')
      return { version, openProject, services, identity: identity(), script: script?.src ?? null }
    })
    note(`editor script ${info.script}; host v${info.version}, services ${JSON.stringify(info.services)}; identity ${JSON.stringify(info.identity)}; url ${page.url()}`)
    note(`requested: ${JSON.stringify(editorRequests.slice(0, 4))}`)
    expect(info.script, 'loaded from the editor base').toBe(editorJs)
    expect(info).toMatchObject({ version: 1, openProject: null, services: { projects: PROJECTS } })
    expect(info.identity.isGuest).toBe(true)
    expect(new URL(page.url()).searchParams.has('editor'), 'Create does not put the flag in the url').toBe(false)
    await expect(nav, 'the menu page covers the HUD').toHaveCount(0)
    expect((await spy(page)).modes, 'listing the scenes changes no mode').toEqual([])
    await expect(page.locator('#dcl-editor-host > *'), 'nothing is docked yet').toHaveCount(0)
    await expect(page.locator('#mygame-canvas')).toHaveCount(1)
    await shot('1-create-page')
  })

  const identityBefore = await page.evaluate(() => (window as GateWindow).__dclEditorHost?.identity().address ?? null).catch(() => null)

  const created = await step('2 New scene on the Create page makes a starter, builds and publishes it under a random preview id, and the engine travels to it', async () => {
    await scenes.getByRole('button', { name: '+ New scene' }).click()
    await scenes.getByText('Example', { exact: true }).click()
    await scenes.locator('.eui-home-modal input').fill(PROJECT_NAME)
    const createdAt = Date.now()
    await scenes.getByRole('button', { name: 'Create scene' }).click()
    await expect.poll(async () => (await spy(page)).openPreview.length, { timeout: 180_000, message: 'host.openPreview is called' }).toBe(1)
    const [called] = (await spy(page)).openPreview
    preview = called.split('@')[0]
    expect(called, 'a random preview id, at the base parcel').toMatch(/^p[0-9a-f]{32}@0,0$/)
    PROJECT_ID = await page.evaluate(async (name) => {
      const root = await navigator.storage.getDirectory()
      const index = await (await (await root.getDirectoryHandle('dcl-editor')).getFileHandle('index.json')).getFile()
      const { projects } = JSON.parse(await index.text()) as { projects: Array<{ id: string; name: string }> }
      return projects.find((project) => project.name === name)?.id ?? ''
    }, PROJECT_NAME)
    expect(PROJECT_ID, 'a new scene gets an id with a random tail').toMatch(/^gate-scene-[a-z0-9]{4}$/)
    await expect(createPage, 'the Create page closed for the editor').toHaveCount(0)
    await expect.poll(async () => (await manifest(page)) != null, { timeout: 60_000, message: 'the preview is published' }).toBe(true)
    const published = (await manifest(page))!
    entityId = published.entity.id
    const machineId = await page.evaluate(() => localStorage.getItem('dcl-editor.machineId'))
    note(`built + published ${Date.now() - createdAt} ms after Create: preview ${preview}, entity ${entityId}, v${published.version}, ${published.entity.content.length} files`)
    expect(Buffer.from(entityId.slice(4), 'base64').toString(), 'a b64- preview id').toBe(`/preview/${preview}-${machineId}`)
    expect(published.entity.content.map((c) => c.file)).toEqual(expect.arrayContaining(['bin/index.js', 'scene.json']))
    expect(
      await page.evaluate(async () => (await (await caches.open('dcl-editor-preview-v1')).keys()).filter((r) => r.url.includes('/preview/gate-scene/')).length),
      'nothing is served under the scene’s name'
    ).toBe(0)
    await expect.poll(() => spawns(createdAt), { timeout: 60_000, message: 'the engine starts the project scene' }).toContain(entityId)
    expect(seen(new RegExp(`realm change .*/preview/${preview}/about`), createdAt), 'by changing realm, in the same page').toBe(true)
    expect(realm(), 'the page url never records the preview realm').toBe(homeRealm)
    expect((await spy(page)).modes[0], 'the editor takes the screen').toBe('edit')
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
    await step('4 a code save is rebuilt and published, reloads the scene by id, and it logs the new marker', async () => {
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
      expect((await spy(page)).canvasKeys, 'what is typed in the editor stays out of the engine').toBe(keysBefore)
      await ui.locator('.eui-studio-hbtn').click()
      await ui.locator('.eui-toolbar').waitFor()
    })

    await step('5 Play then Stop restarts only the project scene, never the HUD scene', async () => {
      const before = await spy(page)
      const playAt = Date.now()
      const run = ui.locator('[data-tip^="Run the scene"]')
      expect(await frozen(), 'paused before Play').toBe(true)
      await sceneTab.click()
      await run.click()
      await expect.poll(async () => (await spy(page)).modes.at(-1), { timeout: 60_000, message: 'the host is in play mode' }).toBe('play')
      await ui.locator('[data-tip^="Scene is running"]').waitFor()
      await expect.poll(frozen, { timeout: 30_000, message: 'the scene runs' }).toBe(false)
      await shot('4-playing')
      await ui.locator('[data-tip="Restart the scene from tick 0"]').click()
      await expect.poll(async () => (await spy(page)).modes.at(-1), { timeout: 60_000, message: 'back in edit mode' }).toBe('edit')
      await run.waitFor({ timeout: 60_000 })
      await expect.poll(frozen, { timeout: 30_000, message: 'paused again after Stop' }).toBe(true)
      const after = await spy(page)
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

  const clockBack = async (): Promise<string> => {
    await page.waitForTimeout(1500)
    const clock = await page.evaluate(() => (window as GateWindow).__dclEditorHost!.engineConsole('time'))
    // the editor stopped the clock at noon to edit: the player has their own back
    expect(clock, 'the clock the player had').toMatch(/-> 20:\d+, speed 7 /)
    return clock
  }
  const card = scenes.locator('.eui-create-card', { hasText: PROJECT_NAME })

  await step('6 Back to scenes leaves the scene for the Create page, and the player is back where they came from', async () => {
    const backAt = Date.now()
    await ui.locator('.eui-topbar-home').click()
    await scenes.locator('.eui-create').waitFor({ timeout: 60_000 })
    await expect(card, 'the Create page lists the scene').toHaveCount(1)
    await expect.poll(() => seen(/GATE_HOME up/, backAt), { timeout: 120_000, message: 'the home scene runs again' }).toBe(true)
    await expect(page.locator('#dcl-editor-host > *'), 'the docked editor is gone').toHaveCount(0)
    await expect(nav, 'the Create page still covers the HUD').toHaveCount(0)
    note(`after Back to scenes: url ${page.url()}; clock ${await clockBack()}; modes ${JSON.stringify((await spy(page)).modes)}`)
    expect((await spy(page)).modes.at(-1)).toBe('off')
    expect(realm()).toBe(homeRealm)
    await shot('5-back-to-scenes')
    // the page's own close gives the HUD back
    await page.locator('header').getByRole('button', { name: 'Close', exact: true }).click()
    await nav.waitFor({ timeout: 30_000 })
    await expect(createPage).toHaveCount(0)
    await shot('5b-back-in-the-hud')
  })

  await step('7 the menu top bar Create item opens the Create page again, without loading the editor again; the scene opens, and Back to Decentraland gives the HUD back', async () => {
    await nav.getByRole('button', { name: 'Settings', exact: true }).click()
    const item = page.locator('header button[data-page="create"]')
    await item.waitFor()
    // the page fades in
    await page.waitForTimeout(600)
    await shot('6-create-in-the-top-bar')
    const loads = editorRequests.filter((url) => url === editorJs).length
    await item.click()
    await card.waitFor({ timeout: 60_000 })
    await expect(page.locator('header button[data-page="settings"][aria-current="page"]'), 'the Settings page closed').toHaveCount(0)
    expect(editorRequests.filter((url) => url === editorJs).length, 'mounted again, not fetched again').toBe(loads)
    await shot('7-create-page-again')
    const reopenedAt = Date.now()
    await card.click()
    await expect.poll(() => spawns(reopenedAt), { timeout: 120_000, message: 'the engine runs the scene again' }).toContain(entityId)
    await ui.locator('.eui-toolbar').waitFor({ timeout: 120_000 })
    await expect.poll(frozen, { timeout: 60_000, message: 'the editor attached to it again' }).toBe(true)
    await expect(async () => {
      await sceneTab.click()
      await expect(ui.locator('.eui-left .eui-title')).toHaveText(PROJECT_NAME, { timeout: 1000 })
    }).toPass({ timeout: 30_000 })
    expect((await spy(page)).openPreview.at(-1), 'the same preview id').toBe(`${preview}@0,0`)
    note(`after re-entry the scene opened again: engine spawned ${JSON.stringify(spawns(reopenedAt))}, editor scenes ${JSON.stringify((await spy(page)).spawned)}`)
    await shot('8-scene-open-again')
    const exitAt = Date.now()
    await ui.locator('.eui-topbar-menu-wrap button[data-tip="Settings"]').click()
    // by position: the button is gone before a locator click has finished with it
    const back = (await ui.getByRole('button', { name: 'Back to Decentraland' }).boundingBox())!
    await page.mouse.click(back.x + back.width / 2, back.y + back.height / 2)
    await nav.waitFor({ timeout: 120_000 })
    await expect.poll(() => seen(/GATE_HOME up/, exitAt), { timeout: 120_000, message: 'the home scene runs again' }).toBe(true)
    await expect(page.locator('#dcl-editor-host > *')).toHaveCount(0)
    await expect(createPage).toHaveCount(0)
    note(`url after Back to Decentraland: ${page.url()}; clock: ${await clockBack()}`)
    expect(realm()).toBe(homeRealm)
    await shot('9-back-in-the-hud')
  })

  await step('8 ?editor=<project> opens that scene directly once in-world, with no list in between', async () => {
    const at = Date.now()
    await page.goto(`${ENTRY}?guest=1&editor=${PROJECT_ID}&realm=${encodeURIComponent(homeRealm)}&position=0,0&${SERVERS}`, { waitUntil: 'commit' })
    await expect.poll(() => seen(/GATE_HOME up/, at), { timeout: 420_000, message: 'the home scene runs' }).toBe(true)
    await expect.poll(() => spawns(at), { timeout: 180_000, message: 'the engine runs the project scene' }).toContain(entityId)
    await ui.locator('.eui-toolbar').waitFor({ timeout: 120_000 })
    await expect(ui.locator('.eui-loading')).toHaveCount(0, { timeout: 60_000 })
    await expect(createPage, 'the Create page closed for the scene').toHaveCount(0)
    expect(await page.evaluate(() => (window as GateWindow).__dclEditorHost!.openProject)).toBe(PROJECT_ID)
    await expect(nav, 'the HUD chrome is hidden in edit mode').toHaveCount(0)
    await expect.poll(() => new URL(page.url()).searchParams.get('editor'), { message: "the engine's url sync keeps the flag" }).toBe(PROJECT_ID)
    await expect(async () => {
      await sceneTab.click()
      await expect(ui.locator('.eui-left .eui-title')).toHaveText(PROJECT_NAME, { timeout: 1000 })
    }).toPass({ timeout: 30_000 })
    note(`?editor=${PROJECT_ID} docked on the scene ${Date.now() - at} ms after navigation`)
    await shot('10-deep-link')
  })

  note(`requests to the project service as a guest: ${JSON.stringify(serviceRequests)}`)
  await testInfo.attach('gate-evidence', { body: evidence.join('\n'), contentType: 'text/plain' })
  expect(failed, 'failed steps').toEqual([])
  expect(serviceRequests, 'a guest sends the project service nothing').toEqual([])
  expect(blocked, 'deployments to real servers').toEqual([])
})

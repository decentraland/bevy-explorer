// The editor gate: the sidebar's Create button opens the HUD's Create page, where the scene editor
// package (a dcl-editor checkout's packages/web/dist, served under /editor/) lists the scenes; a
// scene is created from its starter, built and published in the browser, docked, given a box that
// a real pointer drags by its gizmo, edited, played and stopped in the REAL engine. Back to scenes
// is the Create page again, the menu's Create item opens it too, Back to Decentraland gives the HUD
// back, and `?editor=<project>` opens the scene directly. Nothing serves the scene but the service
// worker, and a guest's editor asks the project service for nothing. Each numbered step passes or
// fails on its own. Run: see playwright.gate.config.ts.

import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test, type Locator, type Page } from '@playwright/test'
import { BRIDGE, HOME, HOME_REALM, NAV, PROJECTS, SCENES, UI, keepOffProduction, previewId } from './gate'
import { cmd, position } from './helpers'

type Point = { x: number; y: number; z: number }

const PROJECT_NAME = 'Gate scene'
const PROJECT_ID = 'gate-scene'
const MARKER = /GATE_MARKER v(\d+)/
const ENTITY_NAME = 'Gate entity'
const COMPOSITE = 'assets/scene/main.composite'
const CREATE_INTRO = 'Build scenes right here. Open one to walk into it and start building.'
const BOX = 'Box'

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

type Arrow = { at: { x: number; y: number }; dir: { x: number; y: number }; pixels: number }

// The red patches that appear in the viewport (between the editor's panels) when the Move tool's
// gizmo is drawn, against the Select tool's view, biggest first.
const redPatches = (page: Page, pngs: [string, string], from: { x: number; y: number }, to: { x: number; y: number }): Promise<Arrow[]> =>
  page.evaluate(
    async ({ pngs, from, to }) => {
      const pixels = async (png: string): Promise<{ data: Uint8ClampedArray; width: number; scale: number }> => {
        const image = new Image()
        image.src = `data:image/png;base64,${png}`
        await image.decode()
        const canvas = document.createElement('canvas')
        canvas.width = image.width
        canvas.height = image.height
        const context = canvas.getContext('2d')!
        context.drawImage(image, 0, 0)
        return { data: context.getImageData(0, 0, image.width, image.height).data, width: image.width, scale: image.width / innerWidth }
      }
      const [a, b] = [await pixels(pngs[0]), await pixels(pngs[1])]
      // not the avatar's orange
      const red = (d: Uint8ClampedArray, i: number): boolean => d[i] > 150 && d[i] - d[i + 1] > 60 && d[i] - d[i + 2] > 60 && Math.abs(d[i + 1] - d[i + 2]) < 50
      const CELL = 6
      const cells = new Map<string, [number, number][]>()
      for (let y = Math.round(from.y * b.scale); y < Math.round(to.y * b.scale); y++) {
        for (let x = Math.round(from.x * b.scale); x < Math.round(to.x * b.scale); x++) {
          const i = (y * b.width + x) * 4
          const change = Math.abs(a.data[i] - b.data[i]) + Math.abs(a.data[i + 1] - b.data[i + 1]) + Math.abs(a.data[i + 2] - b.data[i + 2])
          if (change < 90 || !red(b.data, i) || red(a.data, i)) continue
          const [px, py] = [x / b.scale, y / b.scale]
          const key = `${Math.floor(px / CELL)},${Math.floor(py / CELL)}`
          cells.set(key, [...(cells.get(key) ?? []), [px, py]])
        }
      }
      const patches: Arrow[] = []
      const seen = new Set<string>()
      for (const start of cells.keys()) {
        if (seen.has(start)) continue
        const points: [number, number][] = []
        const queue = [start]
        seen.add(start)
        while (queue.length > 0) {
          const key = queue.pop()!
          points.push(...cells.get(key)!)
          const [cx, cy] = key.split(',').map(Number)
          for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, -1], [1, -1], [-1, 1]]) {
            const next = `${cx + dx},${cy + dy}`
            if (cells.has(next) && !seen.has(next)) {
              seen.add(next)
              queue.push(next)
            }
          }
        }
        if (points.length < 15) continue
        const mx = points.reduce((t, [x]) => t + x, 0) / points.length
        const my = points.reduce((t, [, y]) => t + y, 0) / points.length
        let [sxx, syy, sxy] = [0, 0, 0]
        for (const [x, y] of points) {
          sxx += (x - mx) ** 2
          syy += (y - my) ** 2
          sxy += (x - mx) * (y - my)
        }
        const angle = Math.atan2(2 * sxy, sxx - syy) / 2
        patches.push({ at: { x: mx, y: my }, dir: { x: Math.cos(angle), y: Math.sin(angle) }, pixels: points.length })
      }
      return patches.sort((p, q) => q.pixels - p.pixels)
    },
    { pngs, from, to }
  )

// How many red pixels within 30 px of `at` turn near white (a hovered handle's gold glow) from the
// first view to the second.
const lit = (page: Page, pngs: [string, string], at: { x: number; y: number }): Promise<number> =>
  page.evaluate(
    async ({ pngs, at }) => {
      const views: Uint8ClampedArray[] = []
      for (const png of pngs) {
        const image = new Image()
        image.src = `data:image/png;base64,${png}`
        await image.decode()
        const canvas = document.createElement('canvas')
        canvas.width = image.width
        canvas.height = image.height
        const context = canvas.getContext('2d')!
        context.drawImage(image, 0, 0)
        const scale = image.width / innerWidth
        const r = Math.round(30 * scale)
        views.push(context.getImageData(Math.round(at.x * scale) - r, Math.round(at.y * scale) - r, 2 * r, 2 * r).data)
      }
      const [before, after] = views
      let n = 0
      for (let i = 0; i < before.length; i += 4) if (before[i] - before[i + 1] > 60 && after[i] > 235 && after[i + 1] > 220 && after[i + 2] > 190) n++
      return n
    },
    { pngs, at }
  )

// The gizmo's X arrow on the screen: a red patch the Move tool draws that lights up under the
// pointer, as a hovered handle does. Leaves the pointer on it.
async function xArrow(page: Page, ui: Locator): Promise<Arrow | null> {
  const size = page.viewportSize()!
  const box = (selector: string): Promise<{ x: number; y: number; width: number; height: number } | null> =>
    ui.locator(selector).first().boundingBox({ timeout: 2000 }).catch(() => null)
  const [left, right, top] = [await box('.eui-left'), await box('.eui-right-col'), await box('.eui-toolbar')]
  const from = { x: Math.ceil((left?.x ?? 0) + (left?.width ?? 0)), y: Math.ceil((top?.y ?? 0) + (top?.height ?? 0)) }
  const to = { x: Math.floor(right?.x ?? size.width), y: size.height }
  const shoot = async (): Promise<string> => (await page.screenshot()).toString('base64')
  const view = async (tool: string): Promise<string> => {
    await ui.locator(`[data-tip^="${tool} ("]`).click()
    await page.waitForTimeout(700)
    return shoot()
  }
  const without = await view('Select')
  const drawn = await view('Move')
  const patches = await redPatches(page, [without, drawn], from, to)
  for (const patch of patches.slice(0, 6)) {
    await page.mouse.move(patch.at.x, patch.at.y, { steps: 4 })
    await page.waitForTimeout(500)
    if ((await lit(page, [drawn, await shoot()], patch.at)) >= 40) return patch
  }
  return null
}

// Where the composite the editor autosaved puts the entity named `name`.
async function savedPosition(page: Page, name: string): Promise<Point | null> {
  const text = await storedFile(page, COMPOSITE)
  if (text == null) return null
  const { components } = JSON.parse(text) as { components: { name: string; data: Record<string, { json?: { value?: string; position?: Point } }> }[] }
  const names = components.find((c) => c.name === 'core-schema::Name')?.data ?? {}
  const entity = Object.keys(names).find((e) => names[e].json?.value === name)
  return entity == null ? null : (components.find((c) => c.name === 'core::Transform')?.data[entity]?.json?.position ?? null)
}

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

  const scenes = page.locator(SCENES)
  const createPage = page.locator(HOME)
  let preview = ''

  await step('1 the sidebar Create button opens the Create page, the editor lists the scenes there, and nothing of it loads before the click', async () => {
    // a first visit: the page reloads itself once its service worker is active, then boots the engine
    await page.goto(`/?guest=1&realm=${encodeURIComponent(homeRealm)}&position=0,0&${BRIDGE}`, { waitUntil: 'commit' })
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
    await expect(scenes.getByText(CREATE_INTRO)).toHaveCount(1)
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
    await expect(nav, 'the menu page covers the HUD').toHaveCount(0)
    expect((await spy(page)).modes, 'listing the scenes changes no mode').toEqual([])
    await expect(page.locator('#dcl-editor-host > *'), 'nothing is docked yet').toHaveCount(0)
    await expect(page.locator('#mygame-canvas')).toHaveCount(1)
    await page.waitForTimeout(600)
    await shot('1-create-page-1280x720')
    await page.setViewportSize({ width: 1920, height: 1080 })
    await page.waitForTimeout(1200)
    await shot('1-create-page-1920x1080')
    await page.setViewportSize({ width: 1280, height: 720 })
    await page.waitForTimeout(600)
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
    expect(await previewId(page, PROJECT_ID), 'the id the store keeps for the project').toBe(preview)
    await expect(createPage, 'the Create page closed for the editor').toHaveCount(0)
    await expect.poll(async () => (await manifest(page)) != null, { timeout: 60_000, message: 'the preview is published' }).toBe(true)
    const published = (await manifest(page))!
    entityId = published.entity.id
    const machineId = await page.evaluate(() => localStorage.getItem('dcl-editor.machineId'))
    note(`built + published ${Date.now() - createdAt} ms after Create: preview ${preview}, entity ${entityId}, v${published.version}, ${published.entity.content.length} files`)
    expect(Buffer.from(entityId.slice(4), 'base64').toString(), 'a b64- preview id').toBe(`/preview/${preview}-${machineId}`)
    expect(published.entity.content.map((c) => c.file)).toEqual(expect.arrayContaining(['bin/index.js', 'scene.json']))
    expect(await storedFile(page, 'src/index.ts'), 'the project is stored in the browser').toContain('engine.addEntity()')
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
    await step('3b "Add a box" puts a box in front of the player, and a real pointer drag on the gizmo’s X arrow moves it along X', async () => {
      await sceneTab.click()
      const player = await position(page)
      await ui.getByRole('button', { name: 'Add a box' }).click()
      await expect.poll(() => savedPosition(page, BOX), { timeout: 30_000, message: 'the box is autosaved' }).not.toBeNull()
      const placed = (await savedPosition(page, BOX))!
      const ahead = Math.hypot(placed.x - player.x, placed.z - player.z)
      note(`player at (${player.x.toFixed(2)}, ${player.z.toFixed(2)}); box at (${placed.x.toFixed(2)}, ${placed.y.toFixed(2)}, ${placed.z.toFixed(2)}), ${ahead.toFixed(2)} m away`)
      expect(ahead, 'the box lands a few metres from the player').toBeGreaterThan(1.5)
      expect(ahead).toBeLessThan(5)
      let arrow: Arrow | null = null
      await expect
        .poll(async () => (arrow = await xArrow(page, ui)) != null, { timeout: 60_000, message: 'the gizmo’s X arrow is drawn in the viewport, and lights up under the pointer' })
        .toBe(true)
      await shot('2b-gizmo-on-box')
      const { at, dir, pixels } = arrow!
      note(`X arrow: ${pixels} red px around (${at.x.toFixed(0)}, ${at.y.toFixed(0)}), running (${dir.x.toFixed(2)}, ${dir.y.toFixed(2)}), lit under the pointer`)
      await page.mouse.down()
      await page.mouse.move(at.x + dir.x * 120, at.y + dir.y * 120, { steps: 12 })
      await page.waitForTimeout(300)
      await page.mouse.up()
      await expect
        .poll(async () => Math.abs(((await savedPosition(page, BOX))?.x ?? placed.x) - placed.x), { timeout: 30_000, message: 'the drag moved the box along X' })
        .toBeGreaterThan(0.3)
      const moved = (await savedPosition(page, BOX))!
      note(`dragged 120 px: box (${placed.x.toFixed(3)}, ${placed.z.toFixed(3)}) -> (${moved.x.toFixed(3)}, ${moved.z.toFixed(3)}); dx ${(moved.x - placed.x).toFixed(3)} dz ${(moved.z - placed.z).toFixed(3)}`)
      expect(Math.abs(moved.z - placed.z), 'only along X').toBeLessThan(0.05)
      expect(Math.abs(moved.y - placed.y)).toBeLessThan(0.05)
      await shot('2c-box-dragged')
    })

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
      // running, the starter's code has its cube, next to the box added above
      await expect.poll(() => ui.locator('.eui-left .eui-row', { hasText: 'Box' }).count(), { timeout: 30_000 }).toBeGreaterThanOrEqual(2)
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
    const text = (await card.innerText()).replace(/\s+/g, ' ')
    note(`after Back to scenes: url ${page.url()}; card "${text}"; clock ${await clockBack()}; modes ${JSON.stringify((await spy(page)).modes)}`)
    expect(text).toMatch(/On this device only · opened/)
    expect(text).toContain('Not published')
    expect(text, 'no slug, no preview id').not.toMatch(new RegExp(`${PROJECT_ID}|${preview}`))
    expect((await spy(page)).modes.at(-1)).toBe('off')
    expect(realm()).toBe(homeRealm)
    await shot('5-back-to-scenes-1280x720')
    await page.setViewportSize({ width: 1920, height: 1080 })
    await page.waitForTimeout(1200)
    await shot('5-back-to-scenes-1920x1080')
    await page.setViewportSize({ width: 1280, height: 720 })
    await page.waitForTimeout(600)
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
    const loads = editorRequests.filter((path) => path === '/editor/editor.js').length
    await item.click()
    await card.waitFor({ timeout: 60_000 })
    await expect(page.locator('header button[data-page="settings"][aria-current="page"]'), 'the Settings page closed').toHaveCount(0)
    expect(editorRequests.filter((path) => path === '/editor/editor.js').length, 'mounted again, not fetched again').toBe(loads)
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
    await page.goto(`/?guest=1&editor=${PROJECT_ID}&realm=${encodeURIComponent(homeRealm)}&position=0,0&${BRIDGE}`, { waitUntil: 'commit' })
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

test('?editor=<project> this browser does not have opens the Create page once in-world, and says so', async ({ page }, testInfo) => {
  const lines: string[] = []
  page.on('console', (message) => lines.push(message.text()))
  const homeRealm = `${testInfo.project.use.baseURL!}${HOME_REALM}`
  await page.goto(`/?guest=1&editor=${PROJECT_ID}&realm=${encodeURIComponent(homeRealm)}&position=0,0&${BRIDGE}`, { waitUntil: 'commit' })
  await expect.poll(() => lines.some((line) => /GATE_HOME up/.test(line)), { timeout: 420_000, message: 'the home scene runs' }).toBe(true)
  const scenes = page.locator(SCENES)
  await scenes.getByText(`“${PROJECT_ID}” is not on this device or on your account.`, { exact: false }).waitFor({ timeout: 120_000 })
  expect(await page.evaluate(() => (window as GateWindow).__dclEditorHost!.openProject)).toBe(PROJECT_ID)
  await expect(page.locator(NAV), 'the Create page covers the HUD').toHaveCount(0)
  await expect(page.locator(`${UI} .eui-toolbar`), 'nothing docked').toHaveCount(0)
  await expect.poll(() => new URL(page.url()).searchParams.get('editor'), { message: "the engine's url sync keeps the flag" }).toBe(PROJECT_ID)
})

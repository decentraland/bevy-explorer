// The preview-realm gate: a scene built IN THIS PAGE (the editor's web-build) is served by the
// service worker's preview realm (deploy/web/PREVIEW_REALM.md), the real engine runs it, and after
// an edit + rebuild + '/reload <entityId>' the engine fetches the entity again and runs the new
// bundle. Nothing serves the scene but the service worker. Run: see playwright.gate.config.ts.
// The reload respawns the scene by id: the engine keeps its parcel pointers and re-reads the entity
// from content/contents/<entityId>, so entities/active is asked at launch only.

import { randomBytes } from 'node:crypto'
import { appendFileSync, readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test, type BrowserContext, type Page, type Worker } from '@playwright/test'
import { SERVERS } from './gate'
import { cmd } from './helpers'

// what the editor previews a project under: a random id, never its name
const PROJECT_ID = `p${randomBytes(16).toString('hex')}`
const MARKER = /GATE_MARKER v(\d+)/

interface SceneBuilder {
  ready: Promise<{ sdkVersion: string }>
  build(
    files: Map<string, Uint8Array>,
    options: { production: boolean }
  ): Promise<{ ok: boolean; outputs: Map<string, Uint8Array>; diagnostics: { message: string }[]; ms: number }>
}
interface WebBuild {
  createSceneBuilder(urls: { workerUrl: string; wasmUrl: string; snapshotUrl: string }): SceneBuilder
  previewRealmUrl(pageDir: string, projectId: string): string
  publishPreview(input: {
    realmUrl: string
    projectId: string
    files: Map<string, Uint8Array>
    buildOutputs: Map<string, Uint8Array>
    sceneJson: unknown
    sdkVersion: string
    machineId: string
  }): Promise<{ realmUrl: string; entityId: string }>
}
type GateWindow = Window & { __webBuild?: WebBuild; __gateBuilder?: SceneBuilder }

interface PreviewRequest {
  t: number
  method: string
  url: string
  status: number
  body?: string
}

function readTree(dir: string, prefix = ''): Record<string, string> {
  const out: Record<string, string> = {}
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = prefix + entry.name
    if (entry.isDirectory()) Object.assign(out, readTree(join(dir, entry.name), path + '/'))
    else out[path] = readFileSync(join(dir, entry.name), 'utf8')
  }
  return out
}

// the starter template, logging its version as soon as the bundle is evaluated
function sceneFiles(version: number): Record<string, string> {
  const starter = readTree(join(process.env.WEB_EDITOR_DIR ?? '', 'packages/desktop/templates/starter'))
  return { ...starter, 'src/index.ts': `console.log('GATE_MARKER v${version}')\n${starter['src/index.ts']}` }
}

async function previewWorker(context: BrowserContext): Promise<Worker> {
  const find = (): Worker | undefined => context.serviceWorkers().find((w) => w.url().endsWith('/service_worker.js'))
  await expect.poll(() => find() != null, { message: 'service worker attached' }).toBe(true)
  return find()!
}

// Every request the worker answers from the preview realm, logged inside the worker itself.
async function recordPreviewRequests(worker: Worker): Promise<void> {
  await worker.evaluate(() => {
    type Handle = (request: Request, root: string, store: unknown) => Promise<Response>
    const scope = globalThis as unknown as { dclPreviewRealm: { handle: Handle }; __gateLog?: PreviewRequest[] }
    if (scope.__gateLog) return
    const log: PreviewRequest[] = (scope.__gateLog = [])
    const handle = scope.dclPreviewRealm.handle
    scope.dclPreviewRealm.handle = async (request, root, store) => {
      const body = request.method === 'POST' ? await request.clone().text() : undefined
      const response = await handle(request, root, store)
      log.push({ t: Date.now(), method: request.method, url: request.url, status: response.status, body })
      return response
    }
  })
}

async function previewRequests(worker: Worker): Promise<PreviewRequest[]> {
  const log = await worker.evaluate(() => (globalThis as unknown as { __gateLog?: PreviewRequest[] }).__gateLog ?? null)
  if (log == null) throw new Error('the service worker restarted and lost the request log')
  return log
}

async function loadWebBuild(page: Page): Promise<void> {
  await page.addScriptTag({
    type: 'module',
    content: "import * as webBuild from '/web-build/index.js'; window.__webBuild = webBuild"
  })
  await page.waitForFunction(() => (window as GateWindow).__webBuild != null)
}

interface Built {
  readyMs: number
  buildMs: number
  workerMs: number
  bundleBytes: number
  published: { realm: string; entityId: string; bundleHash: string; publishMs: number } | null
}

// Build in the page (one builder per document); `publish` also rewrites the preview store.
async function build(page: Page, files: Record<string, string>, publish: { machineId: string } | null): Promise<Built> {
  return page.evaluate(
    async ({ files, projectId, publish }) => {
      const w = window as GateWindow
      const webBuild = w.__webBuild!
      const pageDir = new URL(location.pathname.replace(/\/?$/, '/'), location.href).href
      const start = performance.now()
      w.__gateBuilder ??= webBuild.createSceneBuilder({
        workerUrl: `${pageDir}web-build/worker.js`,
        wasmUrl: `${pageDir}web-build/esbuild.wasm`,
        snapshotUrl: `${pageDir}web-build/sdk-snapshot.json`
      })
      const { sdkVersion } = await w.__gateBuilder.ready
      const ready = performance.now()
      const project = new Map(Object.entries(files).map(([path, text]) => [path, new TextEncoder().encode(text)]))
      const result = await w.__gateBuilder.build(project, { production: false })
      const built = performance.now()
      if (!result.ok) throw new Error(`build failed: ${result.diagnostics.map((d) => d.message).join('; ')}`)

      let published: Built['published'] = null
      if (publish) {
        const realmUrl = webBuild.previewRealmUrl(pageDir, projectId)
        const { entityId } = await webBuild.publishPreview({
          realmUrl,
          projectId,
          files: project,
          buildOutputs: result.outputs,
          sceneJson: JSON.parse(files['scene.json']),
          sdkVersion,
          machineId: publish.machineId
        })
        const publishMs = performance.now() - built
        const stored = await (await caches.open('dcl-editor-preview-v1')).match(`${realmUrl}__manifest`)
        const manifest = (await stored!.json()) as { entity: { content: { file: string; hash: string }[] } }
        const bundleHash = manifest.entity.content.find((c) => c.file === 'bin/index.js')!.hash
        // the engine appends /about to the realm: no trailing slash
        published = { realm: realmUrl.replace(/\/$/, ''), entityId, bundleHash, publishMs }
      }
      return {
        readyMs: ready - start,
        buildMs: built - ready,
        workerMs: result.ms,
        bundleBytes: result.outputs.get('bin/index.js')?.byteLength ?? 0,
        published
      }
    },
    { files, projectId: PROJECT_ID, publish }
  )
}

test('a scene built in the browser runs from the preview realm and hot-reloads', async ({ page, context }, testInfo) => {
  expect(process.env.WEB_EDITOR_DIR, 'WEB_EDITOR_DIR').toBeTruthy()
  const consoleLog = testInfo.outputPath('console.log')
  const markers: { t: number; version: number }[] = []
  page.on('console', (message) => {
    const text = message.text()
    appendFileSync(consoleLog, `${new Date().toISOString()} [${message.type()}] ${text}\n`)
    const marker = MARKER.exec(text)
    if (marker) markers.push({ t: Date.now(), version: Number(marker[1]) })
  })
  page.on('pageerror', (error) => appendFileSync(consoleLog, `${new Date().toISOString()} [pageerror] ${error.message}\n`))
  const sawMarker = (version: number, since = 0) => markers.some((m) => m.version === version && m.t >= since)

  // 1. No engine yet: get the page under the service worker, build v1 and fill the store.
  await page.goto('/?mock=1')
  await page.waitForFunction(() => navigator.serviceWorker.controller != null, null, { timeout: 60_000 })
  await page.waitForLoadState('load')
  const worker = await previewWorker(context)
  await recordPreviewRequests(worker)
  await loadWebBuild(page)
  // per run, as a creator's id would be: the engine keys the scene's room on the entity id
  const machineId = `gate${Date.now().toString(36)}`
  const first = await build(page, sceneFiles(1), { machineId })
  const { realm, entityId, bundleHash: bundleV1 } = first.published!
  const about = await page.evaluate(async (url) => {
    const response = await fetch(url)
    return { status: response.status, type: response.headers.get('content-type') }
  }, `${realm}/about`)
  expect(about, 'the realm answers /about from the store').toEqual({ status: 200, type: 'application/json' })

  // 2. The real engine, launched straight into the preview realm.
  const base = (JSON.parse(sceneFiles(1)['scene.json']) as { scene: { base: string } }).scene.base
  const launchedAt = Date.now()
  await page.goto(`/?guest=1&realm=${encodeURIComponent(realm)}&position=${base}&preview&${SERVERS}`)
  await expect.poll(() => sawMarker(1), { timeout: 420_000, message: 'the scene logs its v1 marker' }).toBe(true)
  const v1At = markers.find((m) => m.version === 1)!.t
  // not part of the gate: whether the HUD reaches the world on this realm
  const hudReady = await page
    .waitForSelector('nav[aria-label="Main navigation"]', { timeout: 60_000 })
    .then(() => true)
    .catch(() => false)

  // 3. Edit, rebuild in the engine's page, rewrite the store, reload the one scene.
  await loadWebBuild(page)
  const warm = await build(page, sceneFiles(1), null)
  const second = await build(page, sceneFiles(2), { machineId })
  const bundleV2 = second.published!.bundleHash
  expect(second.published!.entityId, 'the entity id is stable across publishes').toBe(entityId)
  expect(bundleV2, 'the bundle id changes with its bytes').not.toBe(bundleV1)
  const reloadAt = Date.now()
  const reply = await cmd(page, `/reload ${entityId}`)
  await expect.poll(() => sawMarker(2, reloadAt), { timeout: 120_000, message: 'the scene logs its v2 marker' }).toBe(true)
  const v2At = markers.find((m) => m.version === 2)!.t

  // 4. Evidence.
  const requests = await previewRequests(worker)
  const stamp = (t: number) => `${new Date(t).toISOString()} (reload ${t >= reloadAt ? '+' : ''}${t - reloadAt} ms)`
  const active = requests.filter((r) => r.method === 'POST' && r.url === `${realm}/content/entities/active`)
  const wanted = (r: PreviewRequest) => r.status === 200 && (r.body ?? '').includes(`"${base}"`)
  const gets = (hash: string) => requests.filter((r) => r.method === 'GET' && r.url === `${realm}/content/contents/${hash}`)
  const strayKeys = await page.evaluate(async () => {
    const keys = await (await caches.open('ipfs-path-cache-v2')).keys()
    return keys.map((k) => k.url).filter((url) => url.includes('/preview/'))
  })
  const lines = [
    `entity ${entityId}`,
    `realm ${realm}`,
    `first build (cold page): worker ready ${first.readyMs.toFixed(0)} ms, build ${first.buildMs.toFixed(0)} ms (in worker ${first.workerMs.toFixed(0)} ms), ${first.bundleBytes} bytes, publish ${first.published!.publishMs.toFixed(0)} ms`,
    `engine page: worker ready ${warm.readyMs.toFixed(0)} ms, first build ${warm.buildMs.toFixed(0)} ms`,
    `rebuild after the edit: ${second.buildMs.toFixed(0)} ms (in worker ${second.workerMs.toFixed(0)} ms), publish ${second.published!.publishMs.toFixed(0)} ms`,
    `launch -> v1 marker: ${v1At - launchedAt} ms; HUD reached the world: ${hudReady}`,
    `/reload sent ${new Date(reloadAt).toISOString()}, reply: ${reply}`,
    ...active.map((r) => `POST entities/active ${r.status} ${r.body} at ${stamp(r.t)}`),
    `entities/active after the reload: ${active.filter((r) => r.t >= reloadAt).length}`,
    ...gets(entityId).map((r) => `GET entity by id ${r.status} at ${stamp(r.t)}`),
    ...gets(bundleV1).map((r) => `GET bin/index.js v1 ${r.status} at ${stamp(r.t)}`),
    ...gets(bundleV2).map((r) => `GET bin/index.js v2 ${r.status} at ${stamp(r.t)}`),
    ...markers.map((m) => `GATE_MARKER v${m.version} at ${stamp(m.t)}`),
    `reload -> v2 marker: ${v2At - reloadAt} ms`,
    `preview realm requests answered by the worker: ${requests.length}; non-200: ${
      [...new Set(requests.filter((r) => r.status !== 200).map((r) => `${r.status} ${r.method} ${r.url.slice(realm.length)}`))].join(', ') || 'none'
    }`
  ]
  console.log(`\n--- preview realm gate ---\n${lines.join('\n')}\n`)
  await testInfo.attach('gate-evidence', { body: lines.join('\n'), contentType: 'text/plain' })

  expect(reply).toContain('reloaded')
  expect(v1At, 'v1 ran before the reload').toBeLessThan(reloadAt)
  expect(active.some((r) => wanted(r) && r.t < reloadAt), 'entities/active before the reload').toBe(true)
  expect(gets(entityId).some((r) => r.status === 200 && r.t >= reloadAt), 'the entity is fetched again after the reload').toBe(true)
  expect(gets(bundleV2).some((r) => r.status === 200 && r.t >= reloadAt), 'the new bundle is fetched').toBe(true)
  expect(strayKeys, 'nothing of the realm in the engine asset cache').toEqual([])
})

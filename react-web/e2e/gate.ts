// What the scene editor's gates share (sync, publish, npm): a wallet the page finds signed in,
// the page's console, and the editor's own screens driven with real clicks.

import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect, type BrowserContext, type Locator, type Page, type TestInfo } from '@playwright/test'
import { Authenticator, type AuthIdentity } from '../bridge-scene/node_modules/@dcl/crypto/dist/index.js'
import { createUnsafeIdentity } from '../bridge-scene/node_modules/@dcl/crypto/dist/crypto.js'
import { GATE_PORTS } from './ports'

export const HOME_REALM = '/gate-home'
export const HOME_UP = /GATE_HOME up/
export const NAV = 'nav[aria-label="Main navigation"]'
export const UI = '#dcl-editor-host > #editor-ui-host'
/** The Create page's body, where the editor renders its home. */
export const HOME = '#dcl-editor-home'
/** The editor's home (a shadow root inside the Create page's body). */
export const SCENES = `${HOME} #editor-ui-host`
/** The page's entry path: the dev server's root, or `GATE_ENTRY` (a production layout's no-slash entry). */
export const ENTRY = process.env.GATE_ENTRY ?? '/'
/** Where the page loads the editor package from: its own /editor/, or `GATE_EDITOR_BASE` (a CDN). */
export const editorBase = (origin: string): string => process.env.GATE_EDITOR_BASE ?? `${origin}/editor/`
/** playwright.gate.config.ts starts both. */
export const PROJECTS = `http://localhost:${GATE_PORTS.service}`
export const WORLDS = `http://localhost:${GATE_PORTS.worlds}`
/** Points the page at the gate's own servers: its bridge scene (a production build bundles its
 *  own) and its project service. */
export const SERVERS = [
  ...(process.env.GATE_ENTRY ? [] : [`bridgePort=${GATE_PORTS.bridge}`]),
  `editor-projects=${encodeURIComponent(PROJECTS)}`
].join('&')
export const EDITOR_METADATA = { intent: 'dcl:editor:projects', signer: 'dcl:editor' }

export interface EditorHost {
  version: number
  pageDir: string
  openProject: string | null
  services: { projects: string | null; worldsContent: string }
  identity(): { address: string | null; isGuest: boolean }
  engineConsole(line: string): Promise<string>
}
export type GateWindow = Window & { __dclEditorHost?: EditorHost; __dclEditor?: unknown }

export interface Wallet {
  /** lowercase */
  address: string
  identity: AuthIdentity
}

/** A wallet nobody has seen before, with an hour-long identity. */
export async function newWallet(): Promise<Wallet> {
  const owner = createUnsafeIdentity()
  const identity = await Authenticator.initializeAuthChain(owner.address, createUnsafeIdentity(), 60, async (message) =>
    Authenticator.createSignature(owner, message)
  )
  return { address: owner.address.toLowerCase(), identity }
}

/** Every page of `context` finds `wallet` signed in, as after a login on this origin. */
export async function seedWallet(context: BrowserContext, wallet: Wallet): Promise<void> {
  await context.addInitScript(([key, value]) => localStorage.setItem(key, value), [`single-sign-on-${wallet.address}`, JSON.stringify(wallet.identity)])
}

/** Signed-fetch (ADR-44) headers made outside the page, for asking the project service directly. */
export function signedHeaders(wallet: Wallet, method: string, url: string, origin: string, metadata: object = { ...EDITOR_METADATA, origin }): Record<string, string> {
  const timestamp = String(Date.now())
  const meta = JSON.stringify(metadata)
  const chain = Authenticator.signPayload(wallet.identity, [method, new URL(url).pathname, timestamp, meta].join(':').toLowerCase())
  const links = chain.map((link, i) => [`x-identity-auth-chain-${i}`, JSON.stringify(link)])
  return { origin, 'x-identity-timestamp': timestamp, 'x-identity-metadata': meta, ...Object.fromEntries(links) }
}

/** A gate leaves production alone: a deployment to a catalyst or Worlds server that is not local
 *  is aborted and listed in `blocked`, and the engine's analytics are dropped. */
export async function keepOffProduction(context: BrowserContext, blocked: string[]): Promise<void> {
  await context.route('https://api.segment.io/**', (route) => route.abort())
  await context.route(/^https?:\/\/(?!localhost[:/]|127\.0\.0\.1[:/])[^/]+\/(.*\/)?entities\/?(\?.*)?$/, (route) => {
    const request = route.request()
    if (['GET', 'HEAD', 'OPTIONS'].includes(request.method())) return route.continue()
    blocked.push(`${request.method()} ${request.url()}`)
    return route.abort()
  })
}

export interface Device {
  name: string
  page: Page
  /** The editor's UI (a shadow root inside the host's container). */
  ui: Locator
  /** The editor's home on the Create page. */
  home: Locator
  nav: Locator
  lines: { t: number; text: string }[]
  seen(re: RegExp, since?: number): boolean
  /** Every request this page made to `base`, as `METHOD path -> status`. */
  requests(base: string): string[]
  /** What this page sent, other than reads, to hosts that are not local: `METHOD url`, once each. */
  sentAway(): string[]
  shot(name: string): Promise<void>
}

/** Follow a page: its console (also kept in the test's output), its requests, its screenshots. */
export function watch(page: Page, testInfo: TestInfo, name: string): Device {
  const log = testInfo.outputPath(`${name}-console.log`)
  const lines: Device['lines'] = []
  page.on('console', (message) => {
    const text = message.text()
    lines.push({ t: Date.now(), text })
    appendFileSync(log, `${new Date().toISOString()} [${message.type()}] ${text}\n`)
  })
  page.on('pageerror', (error) => appendFileSync(log, `${new Date().toISOString()} [pageerror] ${error.message}\n`))
  const answered: string[] = []
  page.on('response', (response) => {
    const request = response.request()
    if (request.method() !== 'OPTIONS') answered.push(`${request.method()} ${request.url()} -> ${response.status()}`)
  })
  const shots = process.env.GATE_SHOTS ?? testInfo.outputPath('shots')
  mkdirSync(shots, { recursive: true })
  return {
    name,
    page,
    ui: page.locator(UI),
    home: page.locator(SCENES),
    nav: page.locator(NAV),
    lines,
    seen: (re, since = 0) => lines.some((l) => l.t >= since && re.test(l.text)),
    requests: (base) => answered.filter((line) => line.split(' ')[1].startsWith(base)).map((line) => line.replace(base, '')),
    sentAway: () => [
      ...new Set(
        answered
          .map((line) => line.split(' '))
          .filter(([method, url]) => !['GET', 'HEAD'].includes(method) && !['localhost', '127.0.0.1'].includes(new URL(url).hostname))
          .map(([method, url]) => `${method} ${url.split('?')[0]}`)
      )
    ],
    shot: async (shot) => {
      await page.screenshot({ path: join(shots, `${shot}.png`) }).catch(() => {})
    }
  }
}

/** Load the page with `search` and wait until the scene of its realm runs (`up`). A wallet goes
 *  through the welcome screen; `guest=1` in `search` skips it. */
export async function enterWorld(device: Device, search: string, up: RegExp = HOME_UP): Promise<void> {
  const { page } = device
  // a first visit: the page reloads itself once its service worker is active, then boots the engine
  await page.goto(`/?${search}&${SERVERS}`, { waitUntil: 'commit' })
  const jump = page.getByRole('button', { name: /JUMP INTO DECENTRALAND/ })
  await expect(async () => {
    if (!device.seen(up) && (await jump.isVisible()) && (await jump.isEnabled())) await jump.click({ timeout: 2000 }).catch(() => {})
    expect(device.seen(up), `the realm's scene runs (${up})`).toBe(true)
  }).toPass({ timeout: 420_000 })
}

export const homeSearch = (origin: string, extra = ''): string => `realm=${encodeURIComponent(`${origin}${HOME_REALM}`)}&position=0,0${extra}`

/** The sidebar's Create button, up to the editor's home on the Create page. */
export async function openEditor(device: Device): Promise<void> {
  const { page, nav } = device
  const create = nav.getByRole('button', { name: 'Create' })
  await create.waitFor({ timeout: 120_000 })
  await create.click()
  await page.waitForFunction(() => (window as GateWindow).__dclEditorHost != null && (window as GateWindow).__dclEditor != null, null, { timeout: 120_000 })
  await device.home.locator('.eui-create').getByText('Your scenes').waitFor({ timeout: 60_000 })
}

/** The docked editor's Back to scenes: the HUD's Create page lists the scenes again. */
export async function backToScenes(device: Device): Promise<void> {
  await device.ui.locator('.eui-topbar-home').click()
  await device.home.locator('.eui-create').waitFor({ timeout: 60_000 })
}

/** The docked editor's gear menu, Back to Decentraland: the HUD comes back. */
export async function backToDecentraland(device: Device): Promise<void> {
  const { page, ui } = device
  await ui.locator('.eui-topbar-menu-wrap button[data-tip="Settings"]').click()
  // by position: the button is gone before a locator click has finished with it
  const back = (await ui.getByRole('button', { name: 'Back to Decentraland' }).boundingBox())!
  await page.mouse.click(back.x + back.width / 2, back.y + back.height / 2)
  await device.nav.waitFor({ timeout: 120_000 })
}

/** Import on the Create page: a menu where the browser can also pick a folder. */
export async function importZip(device: Device, file: string): Promise<void> {
  const { page, home } = device
  const chooser = page.waitForEvent('filechooser')
  await home.getByRole('button', { name: 'Import', exact: true }).click()
  const zip = home.getByRole('button', { name: 'A .zip file' })
  if (await zip.count()) await zip.click()
  await (await chooser).setFiles(file)
}

/** The random id a project is previewed under (the editor's store, Origin Private File System). */
export const previewId = (page: Page, id: string): Promise<string | null> =>
  page.evaluate(async (id) => {
    try {
      const root = await (await navigator.storage.getDirectory()).getDirectoryHandle('dcl-editor')
      const index = JSON.parse(await (await (await root.getFileHandle('index.json')).getFile()).text()) as { projects: { id: string; previewId?: string }[] }
      return index.projects.find((project) => project.id === id)?.previewId ?? null
    } catch {
      return null
    }
  }, id)

/** false too while a reloaded scene is not pinned yet */
export const frozen = (page: Page): Promise<boolean> =>
  page.evaluate(() => (window as GateWindow).__dclEditorHost!.engineConsole('scene_stats')).then(
    (stats) => stats.includes('frozen'),
    () => false
  )

/** The editor docked on an open project, with its scene paused. */
export async function docked(device: Device): Promise<void> {
  await device.ui.locator('.eui-toolbar').waitFor({ timeout: 180_000 })
  await expect(device.ui.locator('.eui-loading')).toHaveCount(0, { timeout: 60_000 })
  await expect.poll(() => frozen(device.page), { timeout: 60_000, message: `${device.name}: the editor paused the scene` }).toBe(true)
}

/** A scene from the Example starter, made on the Create page and opened. */
export async function newScene(device: Device, name: string): Promise<void> {
  const { home } = device
  await home.getByRole('button', { name: '+ New scene' }).click()
  await home.getByText('Example', { exact: true }).click()
  await home.locator('.eui-home-modal input').fill(name)
  await home.getByRole('button', { name: 'Create scene' }).click()
  await docked(device)
}

/** Put `line` first in src/index.ts with the code editor and save; resolves once the scene the
 *  save restarted has logged `logs` and is paused again. `replace` overwrites the first line. */
export async function saveFirstLine(device: Device, line: string, logs: RegExp, replace: boolean): Promise<void> {
  const { page, ui } = device
  await ui.getByRole('button', { name: 'Code', exact: true }).click()
  await ui.locator('.cm-content').click()
  await page.keyboard.press('ControlOrMeta+Home')
  if (replace) await page.keyboard.press('Shift+ArrowDown')
  await page.keyboard.type(`${line}\n`)
  const at = Date.now()
  await page.keyboard.press('ControlOrMeta+s')
  await expect.poll(() => device.seen(logs, at), { timeout: 60_000, message: `${device.name}: the scene logs ${logs}` }).toBe(true)
  await expect.poll(() => frozen(page), { timeout: 30_000, message: `${device.name}: paused again after the save` }).toBe(true)
  await ui.locator('.eui-studio-filehead', { hasText: 'Saved' }).waitFor()
  await ui.locator('.eui-studio-hbtn').click()
  await ui.locator('.eui-toolbar').waitFor()
}

/** A file of a project as the editor's store keeps it (Origin Private File System). */
export const storedFile = (page: Page, id: string, path: string): Promise<string | null> =>
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
    { id, path }
  )

/** Evidence lines: printed as the gate runs and attached to the test. */
export function notes(testInfo: TestInfo): { note: (text: string) => void; attach: () => Promise<void> } {
  const startedAt = Date.now()
  const evidence: string[] = []
  return {
    note: (text) => {
      const line = `+${((Date.now() - startedAt) / 1000).toFixed(1)}s ${text}`
      evidence.push(line)
      console.log(`[gate] ${line}`)
    },
    attach: () => testInfo.attach('gate-evidence', { body: evidence.join('\n'), contentType: 'text/plain' })
  }
}

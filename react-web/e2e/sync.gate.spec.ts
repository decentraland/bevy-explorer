// The sync gate: one wallet on two devices (two browser contexts, each its own storage and its
// own REAL engine) against the editor's project storage service on :8787. A scene made on the
// first device is listed, downloaded, built and run on the second; an edit there reaches the
// first; an edit on both is a conflict that loses neither version. A second test asks the same
// live service for what it must refuse. Run: see playwright.gate.config.ts.

import { expect, test, type APIRequestContext, type APIResponse } from '@playwright/test'
import { hashV1 } from '../bridge-scene/node_modules/@dcl/hashing/dist/index.js'
import {
  EDITOR_METADATA,
  PROJECTS,
  keepOffProduction,
  docked,
  enterWorld,
  homeSearch,
  newScene,
  newWallet,
  notes,
  openEditor,
  saveFirstLine,
  seedWallet,
  signedHeaders,
  storedFile,
  watch,
  type Device,
  type GateWindow,
  type Wallet
} from './gate'

const PROJECT_NAME = 'Gate sync'
const PROJECT_ID = 'gate-sync'
const COPY_ID = 'gate-sync-this-device'
const CODE = 'src/index.ts'
const marker = (tag: string): string => `console.log('GATE_MARKER ${tag}')`
const logged = (tag: string): RegExp => new RegExp(`GATE_MARKER ${tag}\\b`)

interface Manifest {
  version: number
  files: Record<string, { hash: string; size: number }>
}

// The project service, asked from outside any page as `wallet`.
function service(request: APIRequestContext, origin: string) {
  const call = (wallet: Wallet | null, method: string, path: string, body?: unknown, metadata?: object): Promise<APIResponse> =>
    request.fetch(`${PROJECTS}${path}`, {
      method,
      headers: {
        ...(wallet == null ? { origin } : signedHeaders(wallet, method, `${PROJECTS}${path}`, origin, metadata)),
        ...(body === undefined ? {} : { 'content-type': 'application/json' })
      },
      data: body === undefined ? undefined : JSON.stringify(body)
    })
  return {
    call,
    projects: async (wallet: Wallet): Promise<{ id: string; version: number }[]> =>
      ((await (await call(wallet, 'GET', '/projects')).json()) as { projects: { id: string; version: number }[] }).projects,
    manifest: async (wallet: Wallet, id: string): Promise<Manifest> => (await (await call(wallet, 'GET', `/projects/${id}/manifest`)).json()) as Manifest,
    // a project file as the account holds it
    file: async (wallet: Wallet, id: string, path: string): Promise<string> => {
      const { files } = (await (await call(wallet, 'GET', `/projects/${id}/manifest`)).json()) as Manifest
      return (await call(wallet, 'GET', `/projects/${id}/files/${files[path].hash}`)).text()
    }
  }
}

test('one wallet on two devices: a scene syncs, fast-forwards, and a conflict loses nothing', async ({ browser, request }, testInfo) => {
  const origin = testInfo.project.use.baseURL!
  const { note, attach } = notes(testInfo)
  const wallet = await newWallet()
  const account = service(request, origin)
  const blocked: string[] = []

  const device = async (name: string): Promise<Device> => {
    const context = await browser.newContext({ baseURL: origin })
    await seedWallet(context, wallet)
    await keepOffProduction(context, blocked)
    const page = await context.newPage()
    page.setDefaultTimeout(60_000)
    const it = watch(page, testInfo, name)
    await enterWorld(it, homeSearch(origin))
    await openEditor(it)
    expect(await page.evaluate(() => (window as GateWindow).__dclEditorHost!.identity()), `${name} is in-world as the wallet`).toEqual({
      address: wallet.address,
      isGuest: false
    })
    return it
  }
  const synced = (it: Device): Promise<void> => it.ui.locator('.eui-topbar .eui-autosave', { hasText: 'Synced' }).waitFor({ timeout: 60_000 })
  const manifestPuts = (it: Device): string[] => it.requests(PROJECTS).filter((line) => line.startsWith(`PUT /projects/${PROJECT_ID}/manifest`))

  const a = await device('device-a')
  await test.step('device A: a new scene is copied to the account', async () => {
    await a.ui.getByText('Synced with your account.').waitFor({ timeout: 30_000 })
    expect(await account.projects(wallet), 'the account starts empty').toEqual([])
    await newScene(a, PROJECT_NAME)
    await saveFirstLine(a, marker('v1'), logged('v1'), false)
    await synced(a)
    await expect.poll(async () => (await account.file(wallet, PROJECT_ID, CODE)).split('\n')[0], { message: 'the account holds v1' }).toBe(marker('v1'))
    const { version, files } = await account.manifest(wallet, PROJECT_ID)
    const sent = a.requests(PROJECTS)
    const firstManifest = sent.findIndex((line) => line.startsWith(`PUT /projects/${PROJECT_ID}/manifest`))
    note(`A (${wallet.address}) synced "${PROJECT_NAME}": account version ${version}, ${Object.keys(files).length} files; ${sent.filter((l) => l.startsWith('PUT /blobs/')).length} blob uploads, ${manifestPuts(a).length} manifest writes`)
    expect(sent.slice(0, firstManifest).some((line) => line.startsWith('PUT /blobs/')), 'content first, manifest last').toBe(true)
    expect(sent.every((line) => / -> 2\d\d$/.test(line) || / -> 404$/.test(line)), `the service accepted the host's signatures: ${sent.filter((l) => !/ -> 2\d\d$/.test(l)).join(' | ')}`).toBe(true)
    await a.shot('g2-1-device-a-synced')
  })

  const b = await device('device-b')
  await test.step('device B: the scene is listed from the account, downloaded, built and run', async () => {
    expect(await storedFile(b.page, PROJECT_ID, CODE), 'nothing of it on this device yet').toBeNull()
    const remote = b.ui.locator('.eui-scene-card[data-sync="remote"]')
    await remote.getByText(PROJECT_NAME).waitFor({ timeout: 30_000 })
    await expect(remote.getByText('Not on this device yet. Opening it downloads it.')).toHaveCount(1)
    await b.shot('g2-2-device-b-lists-it')
    const at = Date.now()
    await remote.click()
    await expect.poll(() => b.seen(logged('v1'), at), { timeout: 180_000, message: 'the engine on B runs the downloaded scene' }).toBe(true)
    await docked(b)
    expect((await storedFile(b.page, PROJECT_ID, CODE))!.split('\n')[0]).toBe(marker('v1'))
    const downloads = b.requests(PROJECTS).filter((line) => line.startsWith(`GET /projects/${PROJECT_ID}/files/`))
    note(`B listed it from the account, downloaded ${downloads.length} files and the engine logged v1 ${b.lines.find((l) => l.t >= at && logged('v1').test(l.text))!.t - at} ms after the click`)
    expect(manifestPuts(b), 'opening it changed nothing on the account').toEqual([])
    await b.shot('g2-3-device-b-runs-it')
  })

  let afterB = 0
  await test.step('device B edits; device A gets it by fast-forward', async () => {
    const before = (await account.manifest(wallet, PROJECT_ID)).version
    await saveFirstLine(b, marker('v2'), logged('v2'), true)
    await synced(b)
    await expect.poll(async () => (await account.manifest(wallet, PROJECT_ID)).version, { message: 'B saved a new version' }).toBe(before + 1)
    afterB = before + 1
    const putsOnA = manifestPuts(a).length
    await a.ui.locator('.eui-topbar-home').click()
    const card = a.ui.locator('.eui-scene-card', { hasText: PROJECT_NAME })
    await card.waitFor()
    await a.ui.getByText('Synced with your account.').waitFor({ timeout: 30_000 })
    await a.shot('g2-4-home-with-synced-project')
    const at = Date.now()
    await card.click()
    await expect.poll(() => a.seen(logged('v2'), at), { timeout: 180_000, message: 'the engine on A runs v2' }).toBe(true)
    await docked(a)
    expect((await storedFile(a.page, PROJECT_ID, CODE))!.split('\n')[0]).toBe(marker('v2'))
    expect(manifestPuts(a).length, 'A wrote nothing to get there').toBe(putsOnA)
    expect((await account.manifest(wallet, PROJECT_ID)).version).toBe(afterB)
    note(`B saved v2 (account version ${before} -> ${afterB}); A reopened the scene and ran v2 without writing a version`)
  })

  await test.step('both devices edit: the second gets the choice, and "Keep both" keeps both', async () => {
    await saveFirstLine(a, marker('v3-a'), logged('v3-a'), true)
    await synced(a)
    await expect.poll(async () => (await account.manifest(wallet, PROJECT_ID)).version, { message: "A's edit is on the account" }).toBe(afterB + 1)
    // B still has the scene open on B's own v2
    await saveFirstLine(b, marker('v3-b'), logged('v3-b'), true)
    const dialog = b.ui.getByText('This scene was changed on another device')
    await dialog.waitFor({ timeout: 60_000 })
    await b.shot('g2-5-conflict-dialog')
    const refused = manifestPuts(b).filter((line) => line.endsWith('-> 409'))
    expect(refused.length, "the service refused B's stale save").toBeGreaterThanOrEqual(1)
    expect((await account.file(wallet, PROJECT_ID, CODE)).split('\n')[0], 'the account still holds A').toBe(marker('v3-a'))
    expect((await storedFile(b.page, PROJECT_ID, CODE))!.split('\n')[0], 'nothing on B was replaced').toBe(marker('v3-b'))

    const at = Date.now()
    await b.ui.getByRole('button', { name: 'Keep both', exact: true }).click()
    await dialog.waitFor({ state: 'detached', timeout: 60_000 })
    await expect.poll(async () => (await account.projects(wallet)).map((p) => p.id).sort(), { timeout: 60_000, message: "B's version is a new scene on the account" }).toEqual([PROJECT_ID, COPY_ID])
    await expect.poll(() => storedFile(b.page, PROJECT_ID, CODE).then((text) => text?.split('\n')[0]), { timeout: 60_000, message: "B's scene is now A's version" }).toBe(marker('v3-a'))
    const kept = {
      account: (await account.file(wallet, PROJECT_ID, CODE)).split('\n')[0],
      accountCopy: (await account.file(wallet, COPY_ID, CODE)).split('\n')[0],
      deviceCopy: (await storedFile(b.page, COPY_ID, CODE))!.split('\n')[0]
    }
    note(`conflict: B's save answered 409 (${refused.length}x); after "Keep both": ${JSON.stringify(kept)}; account version ${(await account.manifest(wallet, PROJECT_ID)).version}`)
    expect(kept).toEqual({ account: marker('v3-a'), accountCopy: marker('v3-b'), deviceCopy: marker('v3-b') })
    expect((await account.manifest(wallet, PROJECT_ID)).version, "A's version was not overwritten").toBe(afterB + 1)
    await expect.poll(() => b.seen(logged('v3-a'), at), { timeout: 120_000, message: "the engine on B runs A's version" }).toBe(true)
    await b.shot('g2-6-after-keep-both')
  })

  note(`sent to hosts that are not local, reads aside: ${JSON.stringify([...new Set([...a.sentAway(), ...b.sentAway()])])}; deployments blocked: ${JSON.stringify(blocked)}`)
  expect(blocked).toEqual([])
  await attach()
})

test('the project service refuses a scene, a stranger, a stale save and an unsigned request', async ({ request }, testInfo) => {
  const origin = testInfo.project.use.baseURL!
  const { note, attach } = notes(testInfo)
  const [owner, stranger] = [await newWallet(), await newWallet()]
  const { call } = service(request, origin)
  const answer = async (response: APIResponse): Promise<string> => `${response.status()} ${((await response.json().catch(() => ({}))) as { error?: string }).error ?? ''}`.trim()

  // the owner's project, saved the way the editor saves one: the bytes, then the manifest
  const bytes = Buffer.from("console.log('secret scene')\n")
  const file = { hash: await hashV1(bytes), size: bytes.length }
  const save = (wallet: Wallet, id: string, baseVersion: number | null): Promise<APIResponse> =>
    call(wallet, 'PUT', `/projects/${id}/manifest`, { name: 'Secret', files: { [CODE]: file }, baseVersion })
  const { uploads } = (await (await call(owner, 'POST', '/projects/secret/uploads', { files: [file] })).json()) as { uploads: { url: string; headers: Record<string, string> }[] }
  expect((await request.put(uploads[0].url, { headers: { ...uploads[0].headers, origin }, data: bytes })).status()).toBe(201)
  expect(await answer(await save(owner, 'secret', null))).toBe('200')
  expect(await (await call(owner, 'GET', `/projects/secret/files/${file.hash}`)).text(), 'the owner reads it back').toBe(bytes.toString())

  // what the engine signs for a scene (crates/dcl/src/js/fetch.rs), with the owner's own identity
  const scene = {
    origin,
    sceneId: 'bafkreiscene',
    parcel: '0,0',
    tld: 'org',
    network: 'mainnet',
    isGuest: false,
    realm: { hostname: origin, protocol: 'v3', serverName: 'gate' },
    signer: 'decentraland-kernel-scene'
  }
  const refusals = {
    'a scene (signed by the owner)': await answer(await call(owner, 'GET', '/projects', undefined, scene)),
    "a scene claiming the editor's intent": await answer(await call(owner, 'GET', '/projects', undefined, { ...EDITOR_METADATA, origin, signer: 'decentraland-kernel-scene' })),
    'another intent': await answer(await call(owner, 'GET', '/projects', undefined, { ...EDITOR_METADATA, origin, intent: 'dcl:explorer:comms' })),
    'another origin': await answer(await call(owner, 'GET', '/projects', undefined, { ...EDITOR_METADATA, origin: 'https://evil.example' })),
    unsigned: await answer(await call(null, 'GET', '/projects')),
    "a stranger reads the owner's manifest": await answer(await call(stranger, 'GET', '/projects/secret/manifest')),
    "a stranger reads the owner's file": await answer(await call(stranger, 'GET', `/projects/secret/files/${file.hash}`)),
    "a stranger's manifest names the owner's blob": await answer(await save(stranger, 'mine', null)),
    'the owner saves on version 1': await answer(await save(owner, 'secret', 1)),
    'the owner saves on version 1 again': await answer(await save(owner, 'secret', 1))
  }
  const listed = ((await (await call(stranger, 'GET', '/projects')).json()) as { projects: unknown[] }).projects
  for (const [what, got] of Object.entries(refusals)) note(`${what} -> ${got}`)
  note(`a stranger lists ${listed.length} projects`)
  expect(refusals).toEqual({
    'a scene (signed by the owner)': '403 scene-signed',
    "a scene claiming the editor's intent": '403 scene-signed',
    'another intent': '403 wrong-intent',
    'another origin': '403 origin-not-allowed',
    unsigned: '401 unsigned',
    "a stranger reads the owner's manifest": '404 not-found',
    "a stranger reads the owner's file": '404 not-found',
    "a stranger's manifest names the owner's blob": '422 missing-blobs',
    'the owner saves on version 1': '200',
    'the owner saves on version 1 again': '409 stale'
  })
  expect(listed).toEqual([])
  await attach()
})

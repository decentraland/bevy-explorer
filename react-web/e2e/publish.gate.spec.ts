// The publish gate: signed in, a scene made in the editor is published to a Worlds content server
// (a local stand-in on :8799, named with `?editor-worlds=`). The page's own dialog asks before
// anything is signed; declining publishes nothing; confirming deploys an entity the server checks
// (id, hashes, auth chain), and a visitor's REAL engine then enters that world and runs it.
// Run: see playwright.gate.config.ts.

import { expect, test } from '@playwright/test'
import { Authenticator, type AuthChain } from '../bridge-scene/node_modules/@dcl/crypto/dist/index.js'
import { hashV1 } from '../bridge-scene/node_modules/@dcl/hashing/dist/index.js'
import { WORLDS, keepOffProduction, enterWorld, homeSearch, newScene, newWallet, notes, openEditor, saveFirstLine, seedWallet, watch, type GateWindow } from './gate'

// not a `.dcl.eth` name: the engine reads a realm that ends in one and is not https as a NAME on
// Decentraland's own Worlds server, so a local server's world of that name cannot be entered by url
const WORLD = 'gate.eth'
const PROJECT_NAME = 'Gate world'

interface Entity {
  pointers: string[]
  content: { file: string; hash: string }[]
  metadata: { worldConfiguration?: { name?: string }; display?: { title?: string } }
}
interface DeployedScene {
  entityId: string
  deployer: string
  deploymentAuthChain: AuthChain
  entity: Entity
}

test('a scene is published to a world after the page asks, and the engine runs what was published', async ({ browser, request }, testInfo) => {
  const origin = testInfo.project.use.baseURL!
  const { note, attach } = notes(testInfo)
  const wallet = await newWallet()
  const blocked: string[] = []
  // unlike any earlier run's
  const tag = `GATE_PUBLISHED ${Date.now().toString(36)}`
  const published = new RegExp(`${tag}\\b`)
  const about = (): Promise<number> => request.get(`${WORLDS}/world/${WORLD}/about`).then((response) => response.status())

  const context = await browser.newContext({ baseURL: origin })
  await seedWallet(context, wallet)
  await keepOffProduction(context, blocked)
  const page = await context.newPage()
  page.setDefaultTimeout(60_000)
  const creator = watch(page, testInfo, 'creator')
  const { ui } = creator
  const modal = ui.locator('.eui-publish-modal')
  const confirmation = page.getByRole('dialog').filter({ hasText: 'Publish this scene?' })
  const deployments = (): string[] => creator.requests(WORLDS).filter((line) => line.startsWith('POST /entities '))
  const askToPublish = async (): Promise<string> => {
    await modal.getByText(WORLD).click()
    await modal.getByRole('button', { name: 'Publish', exact: true }).click()
    await confirmation.waitFor({ timeout: 120_000 })
    return (await confirmation.innerText()).replace(/\s+/g, ' ')
  }

  await test.step('signed in, a scene is made and logs its marker in the preview', async () => {
    await enterWorld(creator, homeSearch(origin, `&editor-worlds=${encodeURIComponent(WORLDS)}`))
    await openEditor(creator)
    const info = await page.evaluate(() => {
      const { services, identity } = (window as GateWindow).__dclEditorHost!
      return { worldsContent: services.worldsContent, identity: identity() }
    })
    expect(info).toEqual({ worldsContent: WORLDS, identity: { address: wallet.address, isGuest: false } })
    await newScene(creator, PROJECT_NAME)
    await saveFirstLine(creator, `console.log('${tag}')`, published, false)
    expect(await about(), 'the world has nothing yet').toBe(404)
  })

  let asked = ''
  await test.step('declining the page’s confirmation publishes nothing', async () => {
    await ui.locator('.eui-topbar-publish').click()
    asked = await askToPublish()
    await page.waitForTimeout(400)
    await creator.shot('g4-1-publish-confirmation')
    note(`the page asks: ${asked}`)
    expect(asked).toContain(`World ${WORLD}`)
    expect(asked).toContain(`Scene ${PROJECT_NAME}`)
    expect(asked).toMatch(/Files \d+ \(/)
    expect(asked).toContain(`Server ${new URL(WORLDS).host}`)
    expect(asked.toLowerCase(), 'the wallet, short').toContain(`signed by ${wallet.address.slice(0, 6)}…${wallet.address.slice(-4)}`)
    expect((await confirmation.locator(`[title]`).filter({ hasText: '…' }).getAttribute('title'))?.toLowerCase(), 'and in full').toBe(wallet.address)
    expect(asked).toContain('Signing is free')
    await confirmation.getByRole('button', { name: 'Cancel' }).click()
    await expect(confirmation).toHaveCount(0)
    await modal.getByText(WORLD).waitFor()
    await page.waitForTimeout(1000)
    expect(deployments(), 'nothing was sent').toEqual([])
    expect(await about(), 'the world still has nothing').toBe(404)
    note('declined: no POST /entities, the world still answers 404')
  })

  let scene: DeployedScene
  let bundle = ''
  await test.step('confirming deploys an entity the Worlds server accepts', async () => {
    asked = await askToPublish()
    await confirmation.getByRole('button', { name: 'Sign and publish' }).click()
    await ui.getByText(`${WORLD} is live!`).waitFor({ timeout: 120_000 })
    await creator.shot('g4-2-published')
    expect(deployments()).toEqual(['POST /entities -> 200'])
    const { scenes } = (await (await request.get(`${WORLDS}/world/${WORLD}/scenes`)).json()) as { scenes: DeployedScene[] }
    expect(scenes.length).toBe(1)
    scene = scenes[0]
    const { entity, entityId, deploymentAuthChain } = scene
    const entityFile = await (await request.get(`${WORLDS}/contents/${entityId}`)).body()
    expect(await hashV1(entityFile), 'the entity id is the hash of the entity file').toBe(entityId)
    for (const { file, hash } of entity.content) {
      const bytes = await (await request.get(`${WORLDS}/contents/${hash}`)).body()
      expect(await hashV1(bytes), `${file} is stored under its content hash`).toBe(hash)
      if (file === 'bin/index.js') bundle = hash
    }
    expect(await Authenticator.validateSignature(entityId, deploymentAuthChain, null), 'the auth chain signs the entity id').toMatchObject({ ok: true })
    expect(Authenticator.ownerAddress(deploymentAuthChain).toLowerCase(), 'as the signed-in wallet').toBe(wallet.address)
    expect(scene.deployer).toBe(wallet.address)
    expect(entity.metadata.worldConfiguration?.name).toBe(WORLD)
    expect(asked, 'the confirmation named the file count that was deployed').toContain(`Files ${entity.content.length} (`)
    const files = entity.content.map((item) => item.file).sort()
    expect(files).toEqual(expect.arrayContaining(['bin/index.js', 'scene.json']))
    expect(files.filter((file) => /\.tsx?$|package\.json|lock\.json|tsconfig/.test(file)), 'no sources are deployed').toEqual([])
    expect((await (await request.get(`${WORLDS}/contents/${bundle}`)).text()).includes(tag), 'the deployed bundle holds the marker').toBe(true)
    note(`deployed ${entityId} to ${WORLD}: ${files.length} files (${files.join(', ')}), pointers ${entity.pointers.join(' ')}, chain of ${deploymentAuthChain.length} links by ${scene.deployer}`)
  })

  await test.step('a visitor’s engine enters the published world and runs that bundle', async () => {
    const link = (await ui.locator('.eui-publish-url').innerText()).trim().split(/\s+/).find((word) => word.startsWith(origin))!
    expect(new URL(link).searchParams.get('realm')).toBe(`${WORLDS}/world/${WORLD}`)
    await expect(modal.getByRole('button', { name: 'Open it' })).toHaveCount(1)
    note(`the creator's page sent to hosts that are not local, reads aside: ${JSON.stringify(creator.sentAway())}`)
    await context.close()

    const elsewhere = await browser.newContext({ baseURL: origin })
    await keepOffProduction(elsewhere, blocked)
    const visitor = watch(await elsewhere.newPage(), testInfo, 'visitor')
    const at = Date.now()
    await enterWorld(visitor, `guest=1&${new URL(link).search.slice(1)}&position=${scene.entity.pointers[0]}`, published)
    await visitor.nav.waitFor({ timeout: 120_000 })
    const fetched = visitor.requests(WORLDS)
    note(`a guest opened ${link}: the engine logged the marker ${Date.now() - at} ms after navigation; from the world: ${JSON.stringify(fetched.filter((l) => /about|index|contents/.test(l)).slice(0, 6))}`)
    expect(fetched).toContain(`GET /world/${WORLD}/about -> 200`)
    expect(fetched, 'the engine ran the bundle that was published').toContain(`GET /contents/${bundle} -> 200`)
    expect(new URL(visitor.page.url()).searchParams.get('realm')).toBe(`${WORLDS}/world/${WORLD}`)
    await visitor.page.waitForTimeout(2000)
    await visitor.shot('g4-3-published-world-running')
    note(`the visitor's page sent to hosts that are not local, reads aside: ${JSON.stringify(visitor.sentAway())}`)
    await elsewhere.close()
  })

  note(`deployments to real servers blocked: ${JSON.stringify(blocked)}`)
  expect(blocked).toEqual([])
  await attach()
})

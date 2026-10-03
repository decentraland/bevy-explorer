// The npm gate: a scene whose code needs a package from npm comes in as a .zip, the editor
// resolves the package from the real registry and freezes it, and the scene builds and runs in
// the REAL engine, logging a value only the package's code can make. As a guest: nothing is
// asked of the project service. Run: see playwright.gate.config.ts.

import { execFileSync } from 'node:child_process'
import { cpSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test, type Page } from '@playwright/test'
import { PROJECTS, docked, enterWorld, homeSearch, importZip, keepOffProduction, notes, openEditor, storedFile, watch } from './gate'

const REGISTRY = 'https://registry.npmjs.org'
// Color('rgb(255, 0, 0)').hex(), computed by the `color` package and the four it depends on
const MARKER = /GATE_NPM #FF0000/

// project ids carry a random tail: find one by the name it was made under
const projectIdOf = (page: Page, name: string): Promise<string> =>
  page.evaluate(async (name) => {
    const root = await navigator.storage.getDirectory()
    const index = await (await (await root.getDirectoryHandle('dcl-editor')).getFileHandle('index.json')).getFile()
    const { projects } = JSON.parse(await index.text()) as { projects: Array<{ id: string; name: string }> }
    return projects.find((project) => project.name === name)?.id ?? ''
  }, name)

interface Lock {
  dependencies: Record<string, string>
  packages: Record<string, { integrity: string }>
}

// The editor's starter, asking npm for `color` and logging what it computes, as a .zip.
function packedScene(dir: string): string {
  const scene = join(dir, 'scene')
  cpSync(join(process.env.WEB_EDITOR_DIR ?? '', 'packages/desktop/templates/starter'), scene, { recursive: true })
  const edit = (file: string, change: (text: string) => string): void => writeFileSync(join(scene, file), change(readFileSync(join(scene, file), 'utf8')))
  edit('package.json', (text) => {
    const manifest = JSON.parse(text) as { dependencies: Record<string, string> }
    manifest.dependencies.color = '^4.2.3'
    return JSON.stringify(manifest, null, 2)
  })
  edit('scene.json', (text) => text.replace('Starter Scene', 'Packed scene'))
  edit('src/index.ts', (text) => `import Color from 'color'\nconsole.log('GATE_NPM ' + Color('rgb(255, 0, 0)').hex())\n${text}`)
  const zip = join(dir, 'packed-scene.zip')
  execFileSync('zip', ['-qr', zip, '.'], { cwd: scene })
  return zip
}

test('an imported scene that needs an npm package resolves it, builds and runs', async ({ page }, testInfo) => {
  expect(process.env.WEB_EDITOR_DIR, 'WEB_EDITOR_DIR').toBeTruthy()
  const origin = testInfo.project.use.baseURL!
  const { note, attach } = notes(testInfo)
  page.setDefaultTimeout(60_000)
  const blocked: string[] = []
  await keepOffProduction(page.context(), blocked)
  const device = watch(page, testInfo, 'npm')
  const { home } = device

  await enterWorld(device, `guest=1&${homeSearch(origin)}`)
  await openEditor(device)

  await importZip(device, packedScene(testInfo.outputPath('packed')))
  await home.getByText('Scene imported').waitFor()
  await home.getByText('It uses 1 package from npm (color).', { exact: false }).waitFor()
  await home.getByText(/^Ready: color 4\.\d+\.\d+, and \d+ packages they depend on\.$/).waitFor({ timeout: 120_000 })
  await device.shot('g5-1-imported-with-packages')
  const at = Date.now()
  await home.getByRole('button', { name: 'Open scene' }).click()
  await expect.poll(() => device.seen(MARKER, at), { timeout: 180_000, message: 'the engine runs code from the package' }).toBe(true)
  await docked(device)
  await device.shot('g5-2-running')

  const projectId = await projectIdOf(page, 'Packed scene')
  expect(projectId, 'the import got an id with a random tail').toMatch(/^packed-scene-[a-z0-9]{4}$/)
  const lock = JSON.parse((await storedFile(page, projectId, 'dcl-editor.lock.json'))!) as Lock
  const registry = device.requests(REGISTRY)
  note(`locked ${Object.keys(lock.packages).join(', ')}; ${registry.length} registry requests (${registry.filter((l) => l.includes('.tgz')).length} tarballs); marker ${device.lines.find((l) => l.t >= at && MARKER.test(l.text))!.t - at} ms after Open`)
  expect(Object.keys(lock.dependencies)).toEqual(['color'])
  expect(Object.keys(lock.packages)).toEqual(expect.arrayContaining([expect.stringMatching(/^color@4\./), expect.stringMatching(/^color-convert@/)]))
  expect(Object.values(lock.packages).every((entry) => entry.integrity.startsWith('sha512-')), 'every package is pinned by its sha512').toBe(true)
  expect(registry.every((line) => line.startsWith('GET ') && line.endsWith('-> 200')), `only reads from the registry: ${registry.join(' | ')}`).toBe(true)
  expect(registry.length, 'the real registry was asked').toBeGreaterThan(0)
  expect(device.requests(PROJECTS), 'a guest asks the project service for nothing').toEqual([])
  expect(blocked).toEqual([])
  await attach()
})

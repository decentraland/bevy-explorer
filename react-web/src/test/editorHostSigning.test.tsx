// What the editor host signs for the editor (features/editorHost/host): requests to the project
// storage service and the services a scene's signed fetch reaches, and Worlds deployments, with the signed-in wallet's stored identity, checked
// against the reference implementation of the auth chain.
import { beforeAll, describe, it, expect, vi } from 'vitest'
import { act, render, renderHook, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
// the bridge scene's tree has it: this file is out of `tsc` (tsconfig.json), which runs before that install
import { Authenticator, type AuthChain } from '../../bridge-scene/node_modules/@dcl/crypto'
import { createUnsafeIdentity } from '../../bridge-scene/node_modules/@dcl/crypto/dist/crypto'
import { PopupHost } from '../design'
import { useEditorHost } from '../features/editorHost/EditorHost'
import type { DclEditorHostV1 } from '../features/editorHost/host/host'
import { localSigner, signDeployment, signFetch } from '../features/editorHost/host/sign'
import type { Signer } from '../features/editorHost/host/signer'
import { fakeSession } from './harness'

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  engine_console_command?: (line: string) => Promise<string>
  __dclEditorSigner?: (signer: Signer) => void
}
const w = window as HostWindow
const session = fakeSession()
const host = (): DclEditorHostV1 => w.__dclEditorHost!

const owner = createUnsafeIdentity()
// The entry the auth site leaves in localStorage, and the profile the engine reports for it.
async function signIn(): Promise<void> {
  const identity = await Authenticator.initializeAuthChain(owner.address, createUnsafeIdentity(), 60, async (message) =>
    Authenticator.createSignature(owner, message)
  )
  localStorage.setItem(`single-sign-on-${owner.address.toLowerCase()}`, JSON.stringify(identity))
  session.profile.data = { address: owner.address.toLowerCase(), name: 'Tester', hasClaimedName: false, isGuest: false }
}

describe('editor host signing', () => {
  // the host is published as a guest, the way the Create page loads it
  beforeAll(async () => {
    w.engine_console_command = async () => ''
    // what the signing script does once the page adds it (jsdom runs no scripts)
    new MutationObserver(() => w.__dclEditorSigner?.({ signFetch, signDeployment, localSigner })).observe(document.head, { childList: true })
    const { result } = renderHook(() => useEditorHost('', session))
    await act(async () => void result.current!.load().catch(() => {}))
  })

  it('signs fetches to the project service only, as the signed-in wallet, with metadata the editor cannot set', async () => {
    const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response('{}'))
    for (const url of [
      'https://evil.example/projects',
      'http://localhost:8787.evil.example/projects',
      'http://localhost:8787@evil.example/projects',
      'http://localhost:87870/projects'
    ]) {
      await expect(host().signedFetch(url)).rejects.toThrow('not-allowed')
    }
    await expect(host().signedFetch('http://localhost:8787/projects')).rejects.toThrow('not-signed-in')
    // a stored login for another wallet than the one in-world signs nothing
    const other = createUnsafeIdentity()
    const otherKey = `single-sign-on-${other.address.toLowerCase()}`
    const otherIdentity = await Authenticator.initializeAuthChain(other.address, createUnsafeIdentity(), 60, async (message) =>
      Authenticator.createSignature(other, message)
    )
    localStorage.setItem(otherKey, JSON.stringify(otherIdentity))
    session.profile.data = { address: owner.address.toLowerCase(), name: 'Tester', hasClaimedName: false, isGuest: false }
    await expect(host().signedFetch('http://localhost:8787/projects')).rejects.toThrow('not-signed-in')
    localStorage.removeItem(otherKey)
    expect(fetched).not.toHaveBeenCalled()

    await signIn()
    await host().signedFetch('http://localhost:8787/projects/p1/manifest?x=1', {
      method: 'put',
      body: '{}',
      headers: { 'content-type': 'application/json', 'X-Identity-Metadata': '{"signer":"decentraland-kernel-scene"}', 'x-forwarded-for': '1.2.3.4' }
    })
    const [url, init] = fetched.mock.lastCall!
    const headers = init!.headers as Record<string, string>
    expect(url).toBe('http://localhost:8787/projects/p1/manifest?x=1')
    expect(headers['content-type']).toBe('application/json')
    expect(headers['X-Identity-Metadata']).toBeUndefined()
    expect(headers['x-forwarded-for'], 'only the headers the host allows').toBeUndefined()
    expect(JSON.parse(headers['x-identity-metadata'])).toEqual({ intent: 'dcl:editor:projects', signer: 'dcl:editor', origin: location.origin })
    // what a signed-fetch verifier rebuilds and checks (ADR-44)
    const chain = [0, 1, 2].map((i) => JSON.parse(headers[`x-identity-auth-chain-${i}`]))
    const payload = `put:/projects/p1/manifest?x=1:${headers['x-identity-timestamp']}:${headers['x-identity-metadata']}`.toLowerCase()
    expect(await Authenticator.validateSignature(payload, chain, null)).toEqual({ ok: true, message: undefined })
    fetched.mockRestore()
  })

  it('signs a deployment only once the player has confirmed it in the page', async () => {
    render(<PopupHost />)
    const request = { world: 'boedo.dcl.eth', entityId: `bafkrei${'a'.repeat(52)}`, title: 'My scene', fileCount: 12, bytes: 3_400_000 }

    session.profile.data = null
    await expect(host().signDeployment(request)).rejects.toThrow('not-signed-in')
    expect(screen.queryByRole('dialog')).toBeNull()

    await signIn()
    const declined = expect(host().signDeployment(request)).rejects.toThrow('cancelled')
    const dialog = await screen.findByRole('dialog')
    const short = `${owner.address.slice(0, 6)}…${owner.address.slice(-4)}`
    for (const fact of ['boedo.dcl.eth', 'My scene', '12 (3.2 MB)', 'worlds-content-server.decentraland.org', short]) expect(dialog).toHaveTextContent(fact)
    expect(screen.getByText(short)).toHaveAttribute('title', owner.address)
    // a stray click on the scrim is not an answer
    await userEvent.click(dialog.closest('[tabindex="-1"]')!)
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await declined

    const signed = host().signDeployment(request)
    await userEvent.click(await screen.findByRole('button', { name: 'Sign and publish' }))
    const chain = (await signed) as AuthChain
    expect(await Authenticator.validateSignature(request.entityId, chain, null)).toEqual({ ok: true, message: undefined })
    for (const entityId of ['get:/projects:1:{}', 'QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG', 'a'.repeat(64)]) {
      await expect(host().signDeployment({ ...request, entityId })).rejects.toThrow('invalid entity id')
    }
  })

  it('signs requests to the services a scene reaches as a scene would, path only, and never as the editor', async () => {
    const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response('{}'))
    session.profile.data = null
    await expect(host().signedFetch('https://storage.decentraland.org/players')).rejects.toThrow('not-signed-in')
    await signIn()
    const refused: Array<[string, { method?: string; metadata?: Record<string, unknown> }?]> = [
      // the other environment's, and creators-data's lookalike
      ['https://worlds-content-server.decentraland.zone/world/x/about'],
      ['https://creators-data.decentraland.zone/v2/x', { method: 'POST' }],
      ['https://creators-data.decentraland.org/v3/x', { method: 'POST' }],
      ['https://storage.decentraland.org@evil.example/players'],
      ['https://storage.decentraland.org:444/players'],
      ['https://storage.decentraland.org.evil.example/players'],
      ['https://worlds-content-server.decentraland.org/entities', { method: 'POST' }],
      ['https://creators-data.decentraland.org/v2/x'],
      ['https://multiplayer-server.decentraland.org/logs', { method: 'POST' }],
      ['https://multiplayer-server.decentraland.org/rooms'],
      ['https://storage.decentraland.org/players', { metadata: { signer: 'dcl:editor' } }],
      ['https://storage.decentraland.org/players', { metadata: { intent: 'dcl:editor:projects' } }]
    ]
    for (const [url, init] of refused) await expect(host().signedFetch(url, init), url).rejects.toThrow('not-allowed')
    expect(fetched).not.toHaveBeenCalled()

    for (const [url, method] of [
      ['https://worlds-content-server.decentraland.org/world/boedo.dcl.eth/permissions', 'PUT'],
      ['https://comms-gatekeeper.decentraland.org/scene-admin', 'POST'],
      ['https://storage.decentraland.org/players/0x1', 'DELETE'],
      ['https://creators-data.decentraland.org/v2/assets', 'POST'],
      ['https://multiplayer-server.decentraland.org/logs?since=1', 'GET']
    ]) {
      await host().signedFetch(url, { method })
      expect(fetched.mock.lastCall![0], url).toBe(url)
    }

    const metadata = { signer: 'decentraland-kernel-scene', sceneId: 'bafkreiscene', realm: { hostname: 'boedo.dcl.eth' } }
    const body = new FormData()
    const abort = new AbortController()
    await host().signedFetch('https://storage.decentraland.org/players/0x1/values?key=a', {
      method: 'delete',
      body,
      metadata,
      signal: abort.signal,
      headers: { 'x-confirm-delete-all': 'true' }
    })
    const init = fetched.mock.lastCall![1]!
    const headers = init.headers as Record<string, string>
    expect(init.body).toBe(body)
    expect(init.signal).toBe(abort.signal)
    expect(headers['content-type'], 'the browser sets the form boundary').toBeUndefined()
    expect(headers['x-confirm-delete-all']).toBe('true')
    expect(headers['x-identity-metadata']).toBe(JSON.stringify(metadata))
    // the path without the query, as a scene's signed fetch signs it
    const chain = [0, 1, 2].map((i) => JSON.parse(headers[`x-identity-auth-chain-${i}`]))
    const payload = `delete:/players/0x1/values:${headers['x-identity-timestamp']}:${headers['x-identity-metadata']}`.toLowerCase()
    expect(await Authenticator.validateSignature(payload, chain, null)).toEqual({ ok: true, message: undefined })

    // the project service keeps its own metadata
    await host().signedFetch('http://localhost:8787/projects', { metadata })
    expect(JSON.parse((fetched.mock.lastCall![1]!.headers as Record<string, string>)['x-identity-metadata'])).toMatchObject({ intent: 'dcl:editor:projects' })
    fetched.mockRestore()
  })

  it('unpublishes a scene from a world only once the player has confirmed it in the page', async () => {
    const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response('{}'))
    render(<PopupHost />)
    await signIn()
    const url = 'https://worlds-content-server.decentraland.org/world/boedo.dcl.eth/scenes/-3,4'
    const declined = expect(host().signedFetch(url, { method: 'DELETE' })).rejects.toThrow('cancelled')
    const dialog = await screen.findByRole('dialog')
    for (const fact of ['boedo.dcl.eth', '-3,4', 'worlds-content-server.decentraland.org']) expect(dialog).toHaveTextContent(fact)
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await declined
    expect(fetched).not.toHaveBeenCalled()

    const removed = host().signedFetch(`${url.replace('/scenes/', '/Scenes/')}/`, { method: 'DELETE' })
    await userEvent.click(await screen.findByRole('button', { name: 'Sign and unpublish' }))
    await removed
    expect(fetched.mock.lastCall![0]).toBe(`${url.replace('/scenes/', '/Scenes/')}/`)
    fetched.mockRestore()
  })
})

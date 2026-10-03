// What the editor host signs for the editor (features/editorHost/host): requests to the project
// storage service and Worlds deployments, with the signed-in wallet's stored identity, checked
// against the reference implementation of the auth chain.
import { beforeAll, describe, it, expect, vi } from 'vitest'
import { act, render, renderHook, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
// the bridge scene's tree has it: this file is out of `tsc` (tsconfig.json), which runs before that install
import { Authenticator, type AuthChain } from '../../bridge-scene/node_modules/@dcl/crypto'
import { createUnsafeIdentity } from '../../bridge-scene/node_modules/@dcl/crypto/dist/crypto'
import { PopupHost } from '../design'
import { editorSource } from '../features/editorHost/config'
import type { EditorHostScript } from '../features/editorHost/host/editorHost'
import { guardUrlSync, loadEditor, type DclEditorHostV1 } from '../features/editorHost/host/host'
import { useEditorHost } from '../features/editorHost/useEditorHost'
import { PAGE_DIR } from '../lib/publicUrl'
import { fakeSession } from './harness'

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  __dclEditorHostScript?: (script: EditorHostScript) => void
  engine_console_command?: (line: string) => Promise<string>
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
    const { result } = renderHook(() => useEditorHost(editorSource('', 'localhost', PAGE_DIR), session))
    void result.current!.load().catch(() => {})
    await act(async () => w.__dclEditorHostScript!({ guardUrlSync, loadEditor }))
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
    expect(fetched).not.toHaveBeenCalled()

    await signIn()
    await host().signedFetch('http://localhost:8787/projects/p1/manifest?x=1', {
      method: 'put',
      body: '{}',
      headers: { 'content-type': 'application/json', 'X-Identity-Metadata': '{"signer":"decentraland-kernel-scene"}' }
    })
    const [url, init] = fetched.mock.lastCall!
    const headers = init!.headers as Record<string, string>
    expect(url).toBe('http://localhost:8787/projects/p1/manifest?x=1')
    expect(headers['content-type']).toBe('application/json')
    expect(headers['X-Identity-Metadata']).toBeUndefined()
    expect(JSON.parse(headers['x-identity-metadata'])).toEqual({ intent: 'dcl:editor:projects', signer: 'dcl:editor', origin: location.origin })
    // what a signed-fetch verifier rebuilds and checks (ADR-44)
    const chain = [0, 1, 2].map((i) => JSON.parse(headers[`x-identity-auth-chain-${i}`]))
    const payload = `put:/projects/p1/manifest:${headers['x-identity-timestamp']}:${headers['x-identity-metadata']}`.toLowerCase()
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
})

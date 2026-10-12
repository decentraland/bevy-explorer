// Inside the web shell the app never holds the sign-in key: the editor host asks the shell to sign.

import { beforeAll, describe, it, expect, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'
import { useEditorHost } from '../features/editorHost/EditorHost'
import type { DclEditorHostV1 } from '../features/editorHost/host/host'
import { localSigner, signDeployment, signFetch } from '../features/editorHost/host/sign'
import type { Signer } from '../features/editorHost/host/signer'
import { shellRequest } from '../lib/shell'
import { fakeSession } from './harness'

vi.mock('../lib/shell', () => ({ inShell: true, postToShell: vi.fn(), shellRequest: vi.fn() }))

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  engine_console_command?: (line: string) => Promise<string>
  __dclEditorSigner?: (signer: Signer) => void
}
const w = window as HostWindow
const session = fakeSession()
// the public identity the shell hands out: no private key, and it signs on request
const OWNER = '0x' + '1'.repeat(40)
const EPHEMERAL = '0x' + '2'.repeat(40)
const identity = {
  ephemeralIdentity: { address: EPHEMERAL, publicKey: '0x04' },
  expiration: new Date(Date.now() + 60_000).toISOString(),
  authChain: [
    { type: 'SIGNER', payload: OWNER, signature: '' },
    { type: 'ECDSA_EPHEMERAL', payload: `Ephemeral address: ${EPHEMERAL}`, signature: '0xowner' }
  ]
}

describe('editor host signing inside the web shell', () => {
  beforeAll(async () => {
    w.engine_console_command = async () => ''
    new MutationObserver(() => w.__dclEditorSigner?.({ signFetch, signDeployment, localSigner })).observe(document.head, { childList: true })
    const { result } = renderHook(() => useEditorHost('', session))
    await act(async () => void result.current!.load().catch(() => {}))
  })

  it('asks the shell to sign with the ephemeral key, and reads no key from storage', async () => {
    vi.mocked(shellRequest).mockImplementation(async (method: string) => {
      if (method === 'identity') return { address: OWNER, identity }
      if (method === 'sign') return '0xsigned'
      throw new Error(`unexpected ${method}`)
    })
    session.profile.data = { address: OWNER, name: 'Tester', hasClaimedName: false, isGuest: false }
    const read = vi.spyOn(Storage.prototype, 'getItem')
    const fetched = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response('{}'))

    await w.__dclEditorHost!.signedFetch('http://localhost:8787/projects/p1/manifest', { method: 'PUT', body: '{}' })

    const sign = vi.mocked(shellRequest).mock.calls.find(([method]) => method === 'sign')
    const headers = fetched.mock.lastCall![1]!.headers as Record<string, string>
    const { signer, message } = sign![1] as { signer: string; message: string }
    expect(signer).toBe(EPHEMERAL)
    expect(message).toBe(['put', '/projects/p1/manifest', headers['x-identity-timestamp'], headers['x-identity-metadata']].join(':').toLowerCase())
    expect(JSON.parse(headers['x-identity-auth-chain-2'])).toEqual({ type: 'ECDSA_SIGNED_ENTITY', payload: message, signature: '0xsigned' })
    expect(read.mock.calls.filter(([key]) => String(key).startsWith('single-sign-on-'))).toEqual([])
  })
})

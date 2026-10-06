import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AuthIdentity } from '../features/auth/sso'
import { personalSign } from '../shell/sign'

// web3.js docs key; its address is 0x2c7536E3605D9C16a7a3D7b1898e529396a65c23
const KEY = '0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318'
const EPHEMERAL = '0x2c7536E3605D9C16a7a3D7b1898e529396a65c23'

function identity(signer: string, days: number, privateKey = KEY): AuthIdentity {
  const expiration = new Date(Date.now() + days * 86_400_000).toISOString()
  return {
    ephemeralIdentity: { address: EPHEMERAL, publicKey: '0x04', privateKey },
    expiration,
    authChain: [
      { type: 'SIGNER', payload: signer, signature: '' },
      { type: 'ECDSA_EPHEMERAL', payload: `Decentraland Login\nEphemeral address: ${EPHEMERAL}\nExpiration: ${expiration}`, signature: '0xsig' }
    ]
  }
}

const GUEST = '0x1e9a6ddcb5d0b8c2ffd79372e022d63d053ab1bf'
const ACCOUNT = '0x7e5f4552091a69125d5dfcb7b8c2659029395bdf'

vi.mock('../features/auth/guest', () => ({
  webGuestBackend: {},
  createGuestIdentity: vi.fn(async () => identity(GUEST, 30))
}))

async function shell() {
  vi.resetModules()
  return import('../shell/login')
}

describe('shell login requests', () => {
  beforeEach(() => localStorage.clear())

  it('createGuest keeps the guest with its key and hands the app the public identity', async () => {
    const { request } = await shell()
    const login = (await request('createGuest', undefined)) as { address: string; identity: AuthIdentity; guest?: boolean }
    expect(login.address).toBe(GUEST)
    expect(login.guest).toBe(true)
    expect(login.identity.ephemeralIdentity.privateKey).toBeUndefined()
    const stored = JSON.parse(localStorage.getItem('dcl-guest-identity') ?? 'null') as AuthIdentity
    expect(stored.ephemeralIdentity.privateKey).toBe(KEY)

    // it is now the current login, and the shell signs with it
    expect(await request('identity', undefined)).toEqual(login)
    expect(await request('sign', { signer: EPHEMERAL, message: 'Some data' })).toBe(personalSign(KEY, 'Some data'))
  })

  it('the guest is the stored login when it is the freshest, and survives logout', async () => {
    localStorage.setItem(`single-sign-on-${ACCOUNT}`, JSON.stringify(identity(ACCOUNT, 1)))
    localStorage.setItem('dcl-guest-identity', JSON.stringify(identity(GUEST, 30)))
    const { request } = await shell()
    expect(await request('identity', undefined)).toMatchObject({ address: GUEST, guest: true })

    await request('logout', undefined)
    expect(localStorage.getItem(`single-sign-on-${ACCOUNT}`)).toBeNull()
    expect(localStorage.getItem('dcl-guest-identity')).not.toBeNull()
  })

  it('signing with an account replaces the guest', async () => {
    localStorage.setItem('dcl-guest-identity', JSON.stringify(identity(GUEST, 1)))
    localStorage.setItem(`single-sign-on-${ACCOUNT}`, JSON.stringify(identity(ACCOUNT, 30)))
    localStorage.setItem(`thirdweb_guest_session_id_e1adce863fe287bb6cf0e3fd90bdb77f`, 'session')
    const { request } = await shell()
    expect(await request('identity', undefined)).toMatchObject({ address: ACCOUNT })
    expect(localStorage.getItem('dcl-guest-identity')).not.toBeNull()

    await request('sign', { signer: EPHEMERAL, message: 'hello' })
    expect(localStorage.getItem('dcl-guest-identity')).toBeNull()
    expect(localStorage.getItem('thirdweb_guest_session_id_e1adce863fe287bb6cf0e3fd90bdb77f')).toBeNull()
  })
})

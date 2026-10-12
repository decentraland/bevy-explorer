import { describe, expect, it } from 'vitest'
import { secp256k1 } from '@noble/curves/secp256k1.js'
import { createGuestIdentity, publicKeyAddress, type GuestBackend } from '../features/auth/guest'

const hexToBytes = (hex: string): Uint8Array => Uint8Array.from(hex.match(/../g) ?? [], (b) => parseInt(b, 16))

describe('guest identity', () => {
  it('derives the checksummed address of a public key', () => {
    const publicKey = secp256k1.getPublicKey(hexToBytes('00'.repeat(31) + '01'), false)
    expect(publicKeyAddress(publicKey)).toBe('0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf')
  })

  it('builds an identity the guest wallet signed for a fresh ephemeral key', async () => {
    const calls: string[][] = []
    const backend: GuestBackend = {
      login: async (sessionId) => {
        calls.push(['login', sessionId])
        return { token: 'jwt', walletAddress: '0x1e9a6ddCb5D0B8C2Ffd79372e022d63d053aB1bF' }
      },
      sign: async (token, message) => {
        calls.push(['sign', token, message])
        return '0xsigned'
      }
    }

    const identity = await createGuestIdentity(backend)

    const { address, privateKey } = identity.ephemeralIdentity
    expect(address).toBe(publicKeyAddress(secp256k1.getPublicKey(hexToBytes(privateKey?.slice(2) ?? ''), false)))
    const payload = `Decentraland Login\nEphemeral address: ${address}\nExpiration: ${identity.expiration}`
    expect(identity.authChain).toEqual([
      { type: 'SIGNER', payload: '0x1e9a6ddcb5d0b8c2ffd79372e022d63d053ab1bf', signature: '' },
      { type: 'ECDSA_EPHEMERAL', payload, signature: '0xsigned' }
    ])
    expect(calls).toEqual([
      ['login', expect.stringMatching(/^[0-9a-f]{64}$/)],
      ['sign', 'jwt', payload]
    ])
    const days = (new Date(identity.expiration).getTime() - Date.now()) / 86_400_000
    expect(days).toBeGreaterThan(29.9)
    expect(days).toBeLessThanOrEqual(30)
  })
})

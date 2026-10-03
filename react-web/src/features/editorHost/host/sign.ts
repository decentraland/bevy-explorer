// What the scene editor may have signed with the player's identity: a request to the project
// storage service, and a Worlds deployment. It holds no key: host.ts passes the identity in.

import { sign } from '@noble/secp256k1'
import { keccak_256 } from '@noble/hashes/sha3'
import type { AuthChainLink, AuthIdentity } from '../../auth/sso'

const utf8 = new TextEncoder()
const hex = (bytes: Uint8Array): string => Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')

/** The identity's chain plus `payload` signed by its ephemeral key (an Ethereum personal_sign). */
async function signPayload(identity: AuthIdentity, payload: string): Promise<AuthChainLink[]> {
  const message = utf8.encode(payload)
  const prefix = utf8.encode(`\x19Ethereum Signed Message:\n${message.length}`)
  const prefixed = new Uint8Array(prefix.length + message.length)
  prefixed.set(prefix)
  prefixed.set(message, prefix.length)
  const key = identity.ephemeralIdentity.privateKey.replace(/^0x/, '')
  const [signature, recovery] = await sign(keccak_256(prefixed), key, { der: false, recovered: true })
  return [
    ...identity.authChain,
    { type: 'ECDSA_SIGNED_ENTITY', payload, signature: `0x${hex(signature)}${recovery === 1 ? '1c' : '1b'}` }
  ]
}

/** The signed-fetch (ADR-44) headers for one request to the project storage service. */
export async function signFetch(identity: AuthIdentity, method: string, url: string): Promise<Record<string, string>> {
  const timestamp = String(Date.now())
  // fixed here: the service takes this intent only, and refuses a scene's signed fetch
  const metadata = JSON.stringify({ intent: 'dcl:editor:projects', signer: 'dcl:editor', origin: location.origin })
  const { pathname, search } = new URL(url)
  // the query too: a verifier reading the path as the server sees it includes it
  const payload = [method, `${pathname}${search}`, timestamp, metadata].join(':').toLowerCase()
  const headers: Record<string, string> = { 'x-identity-timestamp': timestamp, 'x-identity-metadata': metadata }
  const chain = await signPayload(identity, payload)
  chain.forEach((link, i) => (headers[`x-identity-auth-chain-${i}`] = JSON.stringify(link)))
  return headers
}

/** The auth chain a Worlds deployment of `entityId` is sent with. */
export const signDeployment: (identity: AuthIdentity, entityId: string) => Promise<AuthChainLink[]> = signPayload

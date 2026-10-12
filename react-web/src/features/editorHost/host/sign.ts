// What the scene editor may have signed with the player's identity: a request to the project
// storage service or to a service a scene's signed fetch reaches, and a Worlds deployment. It
// holds no key: host.ts passes the identity in.

import type { AuthChainLink, AuthIdentity } from '../../auth/sso'
import { personalSign } from '../../../shell/sign'

/** An Ethereum personal_sign of `message` by the ephemeral key `address`, wherever that key is kept. */
export type SignMessage = (address: string, message: string) => Promise<string>

/** Signs with the identity's own key, for a page that holds it (outside the web shell). */
export function localSigner(identity: AuthIdentity): SignMessage {
  return async (_address, message) => {
    const key = identity.ephemeralIdentity.privateKey
    if (key === undefined) throw new Error('not-signed-in')
    return personalSign(key, message)
  }
}

/** The identity's chain plus `payload` signed by its ephemeral key. */
async function signPayload(identity: AuthIdentity, payload: string, sign: SignMessage): Promise<AuthChainLink[]> {
  return [...identity.authChain, { type: 'ECDSA_SIGNED_ENTITY', payload, signature: await sign(identity.ephemeralIdentity.address, payload) }]
}

/** The signed-fetch (ADR-44) headers for one request. Without `metadata`, to the project storage
 *  service; with it, to another service, signed as a scene's signed fetch is. */
export async function signFetch(identity: AuthIdentity, method: string, url: string, sign: SignMessage, metadata?: Record<string, unknown>): Promise<Record<string, string>> {
  const timestamp = String(Date.now())
  const { pathname, search } = new URL(url)
  // the project service takes only this intent and verifies the query too; the others verify the path alone
  const [meta, path] =
    metadata == null
      ? [JSON.stringify({ intent: 'dcl:editor:projects', signer: 'dcl:editor', origin: location.origin }), `${pathname}${search}`]
      : [JSON.stringify(metadata), pathname]
  const payload = [method, path, timestamp, meta].join(':').toLowerCase()
  const headers: Record<string, string> = { 'x-identity-timestamp': timestamp, 'x-identity-metadata': meta }
  const chain = await signPayload(identity, payload, sign)
  chain.forEach((link, i) => (headers[`x-identity-auth-chain-${i}`] = JSON.stringify(link)))
  return headers
}

/** The auth chain a Worlds deployment of `entityId` is sent with. */
export const signDeployment: (identity: AuthIdentity, entityId: string, sign: SignMessage) => Promise<AuthChainLink[]> = signPayload

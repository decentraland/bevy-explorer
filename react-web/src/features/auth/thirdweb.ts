// thirdweb in-app wallet, guest mode: a custodial wallet reached by an opaque session id, the way
// the Unity and Godot clients create persistent guests. The same session id always resolves the
// same wallet. Dependency-free: the page calls it on web, the bridge scene on native.

export const THIRDWEB_CLIENT_ID = 'e1adce863fe287bb6cf0e3fd90bdb77f'
// Where the thirdweb SDK keeps a guest's session id, so a site on this origin that signs a guest
// in through the SDK reaches the same wallet.
export const GUEST_SESSION_KEY = `thirdweb_guest_session_id_${THIRDWEB_CLIENT_ID}`

const API = 'https://api.thirdweb.com'
// The enclave that signs for the wallet; it takes the user's token, not a secret key.
const ENCLAVE = 'https://embedded-wallet.thirdweb.com'

export interface GuestWallet {
  token: string
  walletAddress: string
}

async function post<T>(url: string, headers: Record<string, string>, body: object): Promise<T> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...headers },
    body: JSON.stringify(body)
  })
  if (!res.ok) throw new Error(`thirdweb ${res.status}: ${await res.text()}`)
  return (await res.json()) as T
}

/** Sign the guest in. `headers` are added to the request (native sends an allowlisted Origin). */
export async function guestLogin(sessionId: string, headers: Record<string, string> = {}): Promise<GuestWallet> {
  const r = await post<GuestWallet>(`${API}/v1/auth/complete`, { 'x-client-id': THIRDWEB_CLIENT_ID, ...headers }, {
    method: 'guest',
    sessionId
  })
  return { token: r.token, walletAddress: r.walletAddress }
}

/** personal_sign `message` with the guest's wallet. */
export async function guestSign(token: string, message: string, headers: Record<string, string> = {}): Promise<string> {
  const r = await post<{ signature: string }>(
    `${ENCLAVE}/api/v1/enclave-wallet/sign-message`,
    { 'x-thirdweb-client-id': THIRDWEB_CLIENT_ID, Authorization: `Bearer embedded-wallet-token:${token}`, ...headers },
    { messagePayload: { message, isRaw: false, chainId: 1 } }
  )
  return r.signature
}

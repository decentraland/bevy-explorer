// EIP-191 personal_sign with an ephemeral identity's private key: the signature the engine's
// wallet would make with the same key (deterministic RFC 6979, low-s, r || s || v with v = 27/28).
import { secp256k1 } from '@noble/curves/secp256k1.js'
import { keccak_256 } from '@noble/hashes/sha3.js'
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils.js'

export function personalSign(privateKeyHex: string, message: string): string {
  const body = utf8ToBytes(message)
  const prefix = utf8ToBytes(`\x19Ethereum Signed Message:\n${body.length}`)
  const prefixed = new Uint8Array(prefix.length + body.length)
  prefixed.set(prefix)
  prefixed.set(body, prefix.length)
  const key = hexToBytes(privateKeyHex.trim().replace(/^0x/, ''))
  const bytes = secp256k1.sign(keccak_256(prefixed), key, { prehash: false, format: 'recovered' })
  const sig = secp256k1.Signature.fromBytes(bytes, 'recovered')
  const out = new Uint8Array(65)
  out.set(sig.toBytes('compact'))
  out[64] = 27 + (sig.recovery ?? 0)
  return '0x' + bytesToHex(out)
}

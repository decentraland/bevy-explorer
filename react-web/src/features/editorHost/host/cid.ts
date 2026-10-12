// Decentraland's content hash (@dcl/hashing hashV1), the same as the editor's `sync/cid.ts`: the
// CIDv1 of the bytes as one raw sha2-256 block ("bafkrei…") up to 256 KiB, and above that the
// root of a balanced UnixFS tree over raw 256 KiB leaves ("bafybei…").
const CHUNK = 262144
const FANOUT = 174
const RAW = 0x55
const DAG_PB = 0x70
const ALPHABET = 'abcdefghijklmnopqrstuvwxyz234567'

export const CID_PATTERN = /^baf(?:krei|ybei)[a-z2-7]{52}$/

interface Node {
  cid: Uint8Array
  // bytes of the subtree as stored, which a link carries as Tsize
  stored: number
  fileSize: number
}

function base32(bytes: Uint8Array): string {
  let out = ''
  let buffer = 0
  let bits = 0
  for (const byte of bytes) {
    buffer = (buffer << 8) | byte
    bits += 8
    while (bits >= 5) {
      bits -= 5
      out += ALPHABET[(buffer >>> bits) & 31]
    }
  }
  return bits > 0 ? out + ALPHABET[(buffer << (5 - bits)) & 31] : out
}

async function cid(codec: number, bytes: Uint8Array<ArrayBuffer>): Promise<Uint8Array> {
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))
  const out = new Uint8Array(4 + digest.length)
  out.set([0x01, codec, 0x12, 0x20])
  out.set(digest, 4)
  return out
}

function varint(value: number): number[] {
  const out: number[] = []
  while (value >= 0x80) {
    out.push((value % 0x80) | 0x80)
    value = Math.floor(value / 0x80)
  }
  out.push(value)
  return out
}

// a dag-pb node: one link per child, then the UnixFS file header with their sizes
function parent(children: Node[]): Uint8Array<ArrayBuffer> {
  const out: number[] = []
  const data = [0x08, 0x02, 0x18, ...varint(children.reduce((sum, child) => sum + child.fileSize, 0))]
  for (const child of children) {
    const link = [0x0a, child.cid.length, ...child.cid, 0x12, 0x00, 0x18, ...varint(child.stored)]
    out.push(0x12, ...varint(link.length), ...link)
    data.push(0x20, ...varint(child.fileSize))
  }
  out.push(0x0a, ...varint(data.length), ...data)
  return Uint8Array.from(out)
}

export async function contentCid(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  if (bytes.length <= CHUNK) return 'b' + base32(await cid(RAW, bytes))
  let level: Node[] = []
  for (let offset = 0; offset < bytes.length; offset += CHUNK) {
    const chunk = bytes.subarray(offset, offset + CHUNK)
    level.push({ cid: await cid(RAW, chunk), stored: chunk.length, fileSize: chunk.length })
  }
  while (level.length > 1) {
    const parents: Node[] = []
    for (let i = 0; i < level.length; i += FANOUT) {
      const children = level.slice(i, i + FANOUT)
      const encoded = parent(children)
      parents.push({
        cid: await cid(DAG_PB, encoded),
        stored: encoded.length + children.reduce((sum, child) => sum + child.stored, 0),
        fileSize: children.reduce((sum, child) => sum + child.fileSize, 0)
      })
    }
    level = parents
  }
  return 'b' + base32(level[0].cid)
}

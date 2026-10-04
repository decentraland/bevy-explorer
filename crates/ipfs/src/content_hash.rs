//! Checks downloaded bytes against the content hash they were requested by, so a cache keyed on
//! the hash alone only ever holds what the hash names.
//!
//! Covers the hashes decentraland deployments use (`@dcl/hashing`):
//! - `Qm…`: the sha-256 of the bytes in a CIDv0 wrapper (decentraland's own scheme, not an ipfs
//!   file hash).
//! - `bafkrei…`: CIDv1 of the raw bytes, for files of up to one chunk.
//! - `bafybei…`: CIDv1 of the unixfs file node over 256KiB raw chunks, at most 174 links per
//!   node. Ipfs allows other chunkings, which give other hashes for the same bytes: those are
//!   reported as not matching.

use multihash_codetable::{Code, MultihashDigest};

const CHUNK_SIZE: usize = 262144;
const MAX_LINKS: usize = 174;
const RAW: u8 = 0x55;
const DAG_PB: u8 = 0x70;

pub fn matches_content_hash(hash: &str, bytes: &[u8]) -> bool {
    if hash.starts_with("Qm") {
        let mut multihash = vec![0x12, 0x20];
        multihash.extend(sha256(bytes));
        hash == base58(&multihash)
    } else if hash.starts_with("baf") {
        hash == format!("b{}", base32(&file_cid(bytes)))
    } else {
        false
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Code::Sha2_256
        .digest(bytes)
        .digest()
        .try_into()
        .expect("sha-256 is 32 bytes")
}

// binary CIDv1 with a sha-256 multihash
fn cid(codec: u8, block: &[u8]) -> Vec<u8> {
    let mut cid = vec![0x01, codec, 0x12, 0x20];
    cid.extend(sha256(block));
    cid
}

struct Node {
    cid: Vec<u8>,
    // bytes of file content under this node
    file_size: u64,
    // bytes of all blocks under this node, itself included
    total_size: u64,
}

fn file_cid(bytes: &[u8]) -> Vec<u8> {
    let mut nodes: Vec<Node> = if bytes.is_empty() {
        vec![leaf(bytes)]
    } else {
        bytes.chunks(CHUNK_SIZE).map(leaf).collect()
    };
    while nodes.len() > 1 {
        nodes = nodes.chunks(MAX_LINKS).map(parent).collect();
    }
    nodes.remove(0).cid
}

fn leaf(chunk: &[u8]) -> Node {
    Node {
        cid: cid(RAW, chunk),
        file_size: chunk.len() as u64,
        total_size: chunk.len() as u64,
    }
}

// the dag-pb block of a unixfs file node linking to `children`
fn parent(children: &[Node]) -> Node {
    let file_size = children.iter().map(|child| child.file_size).sum();

    // unixfs Data { Type = File, filesize, blocksizes }
    let mut unixfs = vec![0x08, 0x02, 0x18];
    varint(&mut unixfs, file_size);
    for child in children {
        unixfs.push(0x20);
        varint(&mut unixfs, child.file_size);
    }

    // PBNode { Links, Data }, links first
    let mut block = Vec::new();
    for child in children {
        // PBLink { Hash, Name = "", Tsize }
        let mut link = vec![0x0a];
        varint(&mut link, child.cid.len() as u64);
        link.extend(&child.cid);
        link.extend([0x12, 0x00, 0x18]);
        varint(&mut link, child.total_size);

        block.push(0x12);
        varint(&mut block, link.len() as u64);
        block.extend(link);
    }
    block.push(0x0a);
    varint(&mut block, unixfs.len() as u64);
    block.extend(unixfs);

    Node {
        cid: cid(DAG_PB, &block),
        file_size,
        total_size: block.len() as u64 + children.iter().map(|child| child.total_size).sum::<u64>(),
    }
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

// rfc4648 lowercase, unpadded
fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::new();
    let (mut value, mut bits) = (0u32, 0);
    for byte in bytes {
        value = (value << 8) | *byte as u32;
        bits += 8;
        while bits >= 5 {
            out.push(ALPHABET[(value >> (bits - 5)) as usize & 31] as char);
            bits -= 5;
        }
        value &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[(value << (5 - bits)) as usize & 31] as char);
    }
    out
}

// bitcoin alphabet
fn base58(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    // little-endian base-58 digits
    let mut digits: Vec<u8> = Vec::new();
    for byte in bytes {
        let mut carry = *byte as u32;
        for digit in digits.iter_mut() {
            carry += (*digit as u32) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let leading_zeros = bytes.iter().take_while(|byte| **byte == 0).count();
    std::iter::repeat_n(b'1', leading_zeros)
        .chain(digits.iter().rev().map(|digit| ALPHABET[*digit as usize]))
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // `i % 251` for `len` bytes; expected hashes are from `@dcl/hashing` 1.1.3 (`hashV1`, `hashV0`)
    fn content(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn single_chunk_files() {
        for (len, v1, v0) in [
            (
                0,
                "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku",
                "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n",
            ),
            (
                1000,
                "bafkreicojquuwmy7piqjti3zx3buxh47ya64i2vumxmzr5gwqpnfgsd6nu",
                "QmTcHsGD2NnEbei2GLnpWibHQf7VFbCCh8BwwDsCbizyEc",
            ),
            (
                CHUNK_SIZE,
                "bafkreibruh455iawsviqslif5c7uurdcfdemh22mtnytyzvnzn75kpejxy",
                "QmRgPwRd8PpGwUY6g1yqHvqDMKeR3o5obr1fiNj5Nu3XNM",
            ),
        ] {
            let bytes = content(len);
            assert!(matches_content_hash(v1, &bytes), "{len} {v1}");
            assert!(matches_content_hash(v0, &bytes), "{len} {v0}");
        }
    }

    #[test]
    fn chunked_files() {
        for (len, v1, v0) in [
            (
                CHUNK_SIZE + 1,
                "bafybeiexg2oqkfnj56l7fcmawswqbijt5shq4b5rg6a546uwpkqqzwjioi",
                "QmVUgDpQtVyPdvzgVPV86s8ya9NCJkF54eYVkuzhZ487SZ",
            ),
            (
                600000,
                "bafybeicp64het67shnhxiyl3sg5mylxqop6pnqsqpfecb6pmni2ghoxzom",
                "QmSaH6CewmjdYLgsHPG5K1wx5kY3X3tsEAMnLsELvxVWCX",
            ),
            // a full root node, then one byte into a second level
            (
                MAX_LINKS * CHUNK_SIZE,
                "bafybeihpe5snhzneq7xs53nivmsopto5lrogo3wjynauqylqeym5a3irbm",
                "QmNyHHapXfdFiTvFEQFvag5sjM7fhSV1MRuewiXk5of5m4",
            ),
            (
                MAX_LINKS * CHUNK_SIZE + 1,
                "bafybeib4y7ghw2rq7bracc4xwtxrbzo7cfvagdpte2tmrkgwl6dyard3cm",
                "QmbKbiNfuGWWqJzDf2EEhh5m8rs2huM8St7d88z92FqpjG",
            ),
        ] {
            let bytes = content(len);
            assert!(matches_content_hash(v1, &bytes), "{len} {v1}");
            assert!(matches_content_hash(v0, &bytes), "{len} {v0}");
        }
    }

    #[test]
    fn other_bytes_and_other_hashes_do_not_match() {
        let bytes = content(1000);
        let mut other = bytes.clone();
        other[0] ^= 1;
        for hash in [
            "bafkreicojquuwmy7piqjti3zx3buxh47ya64i2vumxmzr5gwqpnfgsd6nu",
            "QmTcHsGD2NnEbei2GLnpWibHQf7VFbCCh8BwwDsCbizyEc",
        ] {
            assert!(matches_content_hash(hash, &bytes));
            assert!(!matches_content_hash(hash, &other));
        }
        // not a content hash: the url digests of `IpfsType::UrlCached`, preview's `b64-` ids
        assert!(!matches_content_hash("b64-L1VzZXJz", &bytes));
        assert!(!matches_content_hash("", &bytes));
    }
}

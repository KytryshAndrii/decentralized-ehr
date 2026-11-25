use crate::models::{BlockPresence, BlockPresenceType};
use ipfs_cid::models::{Cid, Multihash};

/// CID prefix of the `payload` field: binary CID without the digest.
pub fn cid_prefix(cid: &Cid) -> Vec<u8> {
    let mut bytes = cid.to_bytes();
    bytes.truncate(bytes.len().saturating_sub(cid.multihash.digest.len()));
    bytes
}

pub fn block_presence(cid_bytes: &[u8], presence: BlockPresenceType) -> BlockPresence {
    BlockPresence {
        cid: cid_bytes.to_vec(),
        r#type: presence as i32,
    }
}

/// Block CID of a `payload`, computed from the prefix and the hash of the received bytes.
/// `None` when the prefix is unsupported (e.g. CIDv0 or an unknown hash).
pub fn block_cid(prefix: &[u8], data: &[u8]) -> Option<Cid> {
    let (_version, rest) = unsigned_varint::decode::u64(prefix).ok()?;
    let (codec, rest) = unsigned_varint::decode::u64(rest).ok()?;
    let (hash_code, _) = unsigned_varint::decode::u64(rest).ok()?;
    let cid = Cid::new_cid(codec, Multihash::compute(data, hash_code).ok()?);
    // Comparing the whole prefix also checks the CID version and digest length.
    (cid_prefix(&cid) == prefix).then_some(cid)
}

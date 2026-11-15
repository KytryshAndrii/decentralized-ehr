use crate::models::{BlockPresence, BlockPresenceType};
use ipfs_cid::models::Cid;

/// Prefiks CID z pola `payload`: binarny CID bez digestu.
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

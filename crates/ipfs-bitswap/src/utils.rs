use crate::models::{BlockPresence, BlockPresenceType};
use ipfs_cid::models::{Cid, Multihash};

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

/// CID bloku z `payload`, wyliczony z prefiksu i skrótu odebranych bajtów.
/// `None`, gdy prefiks jest nieobsługiwany (np. CIDv0 albo nieznany hash).
pub fn block_cid(prefix: &[u8], data: &[u8]) -> Option<Cid> {
    let (_version, rest) = unsigned_varint::decode::u64(prefix).ok()?;
    let (codec, rest) = unsigned_varint::decode::u64(rest).ok()?;
    let (hash_code, _) = unsigned_varint::decode::u64(rest).ok()?;
    let cid = Cid::new_cid(codec, Multihash::compute(data, hash_code).ok()?);
    // Porównanie całego prefiksu sprawdza też wersję CID i długość skrótu.
    (cid_prefix(&cid) == prefix).then_some(cid)
}

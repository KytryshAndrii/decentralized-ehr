use crate::consts::MAX_BLOCK_BYTES;
use crate::error::StoreError;
use ipfs_cid::consts::{CID_V1, SHA2_256, SHA2_512};
use ipfs_cid::models::{Cid, Multihash};

pub fn validate_block_size(bytes: &[u8]) -> Result<(), StoreError> {
    if bytes.len() > MAX_BLOCK_BYTES {
        return Err(StoreError::BlockTooLarge {
            actual: bytes.len(),
            limit: MAX_BLOCK_BYTES,
        });
    }
    Ok(())
}

pub fn validate_cid(cid: &Cid) -> Result<(), StoreError> {
    if cid.version != CID_V1 {
        return Err(StoreError::UnsupportedCidVersion(cid.version));
    }
    let expected_digest_length = match cid.multihash.code {
        SHA2_256 => 32,
        SHA2_512 => 64,
        other => return Err(StoreError::UnsupportedHash(other)),
    };
    let actual_digest_length = cid.multihash.digest.len();
    if actual_digest_length != expected_digest_length
        || usize::from(cid.multihash.size) != expected_digest_length
    {
        return Err(StoreError::InvalidDigestLength {
            code: cid.multihash.code,
            expected: expected_digest_length,
            actual: actual_digest_length,
        });
    }
    Ok(())
}

/// Prefiks stanowi naszą lokalną przestrzeń nazw LevelDB, a za nim idzie
/// kompletny multihash
pub fn get_multihash_key(multihash: &Multihash) -> Vec<u8> {
    let mut key = b"/blocks/".to_vec();
    let mut buf = unsigned_varint::encode::u64_buffer();
    key.extend_from_slice(unsigned_varint::encode::u64(multihash.code, &mut buf));
    key.extend_from_slice(unsigned_varint::encode::u64(
        multihash.digest.len() as u64,
        &mut buf,
    ));
    key.extend_from_slice(&multihash.digest);
    key
}

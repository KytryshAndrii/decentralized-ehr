use crate::consts::{
    MAX_BLOCK_BYTES, PEER_DHT_CLIENT_MARKER, PEER_DHT_SERVER_MARKER, PEER_SEEN_FIELD,
    PROVIDER_EXPIRES_FIELD,
};
use crate::error::StoreError;
use crate::models::{KnownPeer, Namespace, RemoteProvider};
use ipfs_cid::consts::{CID_V1, SHA2_256, SHA2_512};
use ipfs_cid::models::{Cid, Multihash};
use std::time::{SystemTime, UNIX_EPOCH};

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

pub fn multihash_bytes(multihash: &Multihash) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(multihash.digest.len() + 4);
    let mut varint_buffer = unsigned_varint::encode::u64_buffer();
    bytes.extend_from_slice(unsigned_varint::encode::u64(
        multihash.code,
        &mut varint_buffer,
    ));
    bytes.extend_from_slice(unsigned_varint::encode::u64(
        multihash.digest.len() as u64,
        &mut varint_buffer,
    ));
    bytes.extend_from_slice(&multihash.digest);
    bytes
}

pub fn get_multihash_key(multihash: &Multihash) -> Vec<u8> {
    get_namespaced_multihash_key(Namespace::Blocks, multihash)
}

pub fn get_namespaced_multihash_key(namespace: Namespace, multihash: &Multihash) -> Vec<u8> {
    namespace.key(&multihash_bytes(multihash))
}

/// Peer entry key: `Peers` namespace + PeerId text.
pub fn get_peer_key(peer_id: &str) -> Vec<u8> {
    Namespace::Peers.key(peer_id.as_bytes())
}

/// PeerIds and addresses go into a line format, so they cannot contain whitespace.
fn is_token(value: &str) -> bool {
    !value.is_empty() && !value.contains(char::is_whitespace)
}

pub fn validate_known_peer(peer: &KnownPeer) -> Result<(), StoreError> {
    if !is_token(&peer.peer_id) {
        return Err(StoreError::InvalidPeer("pusty lub błędny PeerId".into()));
    }
    if let Some(address) = peer
        .addresses
        .iter()
        .find(|address| !is_token(address.as_str()))
    {
        return Err(StoreError::InvalidPeer(format!(
            "błędny adres: {address:?}"
        )));
    }
    Ok(())
}

pub fn encode_known_peer(peer: &KnownPeer) -> Vec<u8> {
    let marker = if peer.dht_server {
        PEER_DHT_SERVER_MARKER
    } else {
        PEER_DHT_CLIENT_MARKER
    };
    let seen = format!("{PEER_SEEN_FIELD}{}", peer.last_seen_utc);
    let mut lines = vec![marker, seen.as_str()];
    lines.extend(peer.addresses.iter().map(String::as_str));
    lines.join("\n").into_bytes()
}

pub fn decode_known_peer(peer_id: &str, value: &[u8]) -> Result<KnownPeer, StoreError> {
    let corrupt = || StoreError::CorruptRecord(format!("peer {peer_id}"));
    let text = std::str::from_utf8(value).map_err(|_| corrupt())?;
    let mut lines = text.lines().peekable();
    let dht_server = match lines.next() {
        Some(PEER_DHT_SERVER_MARKER) => true,
        Some(PEER_DHT_CLIENT_MARKER) => false,
        _ => return Err(corrupt()),
    };
    let last_seen_utc = match lines.next_if(|line| line.starts_with(PEER_SEEN_FIELD)) {
        Some(line) => line[PEER_SEEN_FIELD.len()..]
            .parse()
            .map_err(|_| corrupt())?,
        None => utc_now_secs(),
    };
    Ok(KnownPeer {
        peer_id: peer_id.to_string(),
        addresses: lines.map(str::to_string).collect(),
        dht_server,
        last_seen_utc,
    })
}

/// Current time as seconds since the Unix epoch. The epoch is defined in UTC,
/// so the result does not depend on the device time zone.
pub fn utc_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// Length of the canonical multihash at the start of `bytes` (varint code + varint length + digest).
pub fn multihash_prefix_len(bytes: &[u8]) -> Option<usize> {
    let (_, rest) = unsigned_varint::decode::u64(bytes).ok()?;
    let (digest_len, rest) = unsigned_varint::decode::u64(rest).ok()?;
    let digest_len = usize::try_from(digest_len).ok()?;
    if digest_len == 0 || rest.len() < digest_len {
        return None;
    }
    Some(bytes.len() - rest.len() + digest_len)
}

pub fn validate_remote_provider(provider: &RemoteProvider) -> Result<(), StoreError> {
    if multihash_prefix_len(&provider.multihash) != Some(provider.multihash.len()) {
        return Err(StoreError::InvalidProvider("błędny multihash".into()));
    }
    if !is_token(&provider.peer_id) {
        return Err(StoreError::InvalidProvider(
            "pusty lub błędny PeerId".into(),
        ));
    }
    if let Some(address) = provider
        .addresses
        .iter()
        .find(|address| !is_token(address.as_str()))
    {
        return Err(StoreError::InvalidProvider(format!(
            "błędny adres: {address:?}"
        )));
    }
    Ok(())
}

/// Record key: `RemoteProviders` namespace + multihash + PeerId text.
/// The multihash encodes its own length, so the key splits unambiguously.
pub fn get_remote_provider_key(multihash: &[u8], peer_id: &str) -> Vec<u8> {
    let mut id = multihash.to_vec();
    id.extend_from_slice(peer_id.as_bytes());
    Namespace::RemoteProviders.key(&id)
}

pub fn encode_remote_provider(provider: &RemoteProvider) -> Vec<u8> {
    let mut lines = vec![format!(
        "{PROVIDER_EXPIRES_FIELD}{}",
        provider.expires_at_utc
    )];
    lines.extend(provider.addresses.iter().cloned());
    lines.join("\n").into_bytes()
}

pub fn decode_remote_provider(key: &[u8], value: &[u8]) -> Result<RemoteProvider, StoreError> {
    let corrupt = || StoreError::CorruptRecord("rekord providera".into());
    let id = key
        .strip_prefix(Namespace::RemoteProviders.prefix())
        .ok_or_else(corrupt)?;
    let multihash_len = multihash_prefix_len(id).ok_or_else(corrupt)?;
    let (multihash, peer_id) = id.split_at(multihash_len);
    let peer_id = std::str::from_utf8(peer_id).map_err(|_| corrupt())?;
    let text = std::str::from_utf8(value).map_err(|_| corrupt())?;
    let mut lines = text.lines();
    let expires_at_utc = lines
        .next()
        .and_then(|line| line.strip_prefix(PROVIDER_EXPIRES_FIELD))
        .and_then(|secs| secs.parse().ok())
        .ok_or_else(corrupt)?;
    Ok(RemoteProvider {
        multihash: multihash.to_vec(),
        peer_id: peer_id.to_string(),
        addresses: lines.map(str::to_string).collect(),
        expires_at_utc,
    })
}

//! did:key z klucza węzła Ed25519: `did:key:z` + base58btc(`0xed 0x01` + 32 bajty).
//! Ten sam klucz daje PeerId, więc tożsamość sieciowa i autoryzacyjna to jeden podmiot.
use crate::consts::{DID_KEY_PREFIX, ED25519_MULTICODEC};
use crate::error::SecurityError;
use libp2p::PeerId;
use libp2p::identity::{PublicKey, ed25519};

pub fn did_from_public(public: &PublicKey) -> Result<String, SecurityError> {
    let key = public
        .clone()
        .try_into_ed25519()
        .map_err(|_| SecurityError::Key("node key is not Ed25519".into()))?;
    let mut bytes = ED25519_MULTICODEC.to_vec();
    bytes.extend_from_slice(&key.to_bytes());
    Ok(format!(
        "{DID_KEY_PREFIX}{}",
        bs58::encode(bytes).into_string()
    ))
}

pub fn public_from_did(did: &str) -> Result<PublicKey, SecurityError> {
    let invalid = || SecurityError::InvalidDid(did.to_owned());
    let encoded = did.strip_prefix(DID_KEY_PREFIX).ok_or_else(invalid)?;
    let bytes = bs58::decode(encoded).into_vec().map_err(|_| invalid())?;
    let key = bytes
        .strip_prefix(&ED25519_MULTICODEC[..])
        .ok_or_else(invalid)?;
    let key = ed25519::PublicKey::try_from_bytes(key).map_err(|_| invalid())?;
    Ok(PublicKey::from(key))
}

pub fn peer_id_from_did(did: &str) -> Result<PeerId, SecurityError> {
    Ok(PeerId::from(public_from_did(did)?))
}

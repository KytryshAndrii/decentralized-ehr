//! OpenPGP (RFC 9580, rPGP): the issuer's signature inside the ciphertext, hybrid encryption
//! (random AES-256 session key, SEIPD v1) wrapped for every recipient. GnuPG reads the format.
use crate::error::SecurityError;
use crate::models::Identity;
use pgp::composed::{Deserializable, Message, MessageBuilder, SignedPublicKey};
use pgp::crypto::hash::HashAlgorithm;
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::types::KeyDetails;

pub fn parse_public_key(armored: &str) -> Result<SignedPublicKey, SecurityError> {
    let (key, _) = SignedPublicKey::from_armor_single(armored.as_bytes())?;
    key.verify_bindings()?;
    Ok(key)
}

/// OpenPGP key fingerprint (hex), e.g. to show when checking a contact card.
pub fn fingerprint(key: &SignedPublicKey) -> String {
    format!("{:X}", key.fingerprint())
}

/// Signs with `signer` and encrypts for each of `recipients`.
/// shortcut: the whole file is in memory (MessageBuilder from bytes); for files over ~1 GB move to from_reader.
pub fn encrypt_and_sign(
    plaintext: &[u8],
    signer: &Identity,
    recipients: &[&SignedPublicKey],
) -> Result<Vec<u8>, SecurityError> {
    if recipients.is_empty() {
        return Err(SecurityError::Key("no recipients".into()));
    }
    let mut rng = rand::thread_rng();
    let mut builder = MessageBuilder::from_bytes("", plaintext.to_vec())
        .seipd_v1(&mut rng, SymmetricKeyAlgorithm::AES256);
    for recipient in recipients {
        let subkey = recipient
            .public_subkeys
            .iter()
            .find(|subkey| subkey.algorithm().can_encrypt())
            .ok_or_else(|| SecurityError::Key("recipient key cannot encrypt".into()))?;
        builder.encrypt_to_key(&mut rng, subkey)?;
    }
    builder.sign(
        &signer.pgp_secret.primary_key,
        signer.password.as_str().into(),
        HashAlgorithm::Sha256,
    );
    Ok(builder.to_vec(&mut rng)?)
}

/// Decrypts with the own key and requires a valid signature of `expected_signer`.
pub fn decrypt_and_verify(
    ciphertext: &[u8],
    me: &Identity,
    expected_signer: &SignedPublicKey,
) -> Result<Vec<u8>, SecurityError> {
    let mut message = Message::from_bytes(ciphertext)?
        .decrypt(&me.password.as_str().into(), &me.pgp_secret)
        .map_err(|_| SecurityError::DocumentRejected("not encrypted for this key"))?;
    if message.is_compressed() {
        message = message.decompress()?;
    }
    let plaintext = message.as_data_vec()?;
    message.verify(expected_signer).map_err(|_| {
        SecurityError::DocumentRejected("signature of the issuer is missing or invalid")
    })?;
    Ok(plaintext)
}

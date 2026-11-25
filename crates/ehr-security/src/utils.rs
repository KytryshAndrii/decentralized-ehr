use crate::error::SecurityError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn base64_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn base64_decode(text: &str) -> Result<Vec<u8>, SecurityError> {
    URL_SAFE_NO_PAD
        .decode(text.trim())
        .map_err(|error| SecurityError::Encoding(error.to_string()))
}

/// Random 128-bit id for `nonce` values and invitation codes.
pub fn random_id() -> String {
    base64_encode(&rand::random::<[u8; 16]>())
}

use crate::error::SecurityError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(text: &str) -> Result<Vec<u8>, SecurityError> {
    URL_SAFE_NO_PAD
        .decode(text.trim())
        .map_err(|error| SecurityError::Encoding(error.to_string()))
}

/// Losowy identyfikator (128 bitów) dla `nonce` i kodów zaproszeń.
pub fn random_id() -> String {
    b64(&rand::random::<[u8; 16]>())
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("openpgp: {0}")]
    Pgp(#[from] pgp::errors::Error),
    #[error("invalid DID: {0}")]
    InvalidDid(String),
    #[error("invalid encoding: {0}")]
    Encoding(String),
    #[error("signature does not match the signer")]
    BadSignature,
    #[error("wrong password or damaged key file")]
    WrongPassword,
    #[error("password must have at least {0} characters")]
    WeakPassword(usize),
    #[error("keys already exist in {0}")]
    KeysExist(String),
    #[error("no keys in {0}; create a profile first")]
    NoKeys(String),
    #[error("token rejected: {0}")]
    TokenRejected(&'static str),
    #[error("document rejected: {0}")]
    DocumentRejected(&'static str),
    #[error("key error: {0}")]
    Key(String),
}

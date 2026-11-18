use thiserror::Error;

#[derive(Debug, Error)]
pub enum EhrError {
    #[error(transparent)]
    Security(#[from] ehr_security::SecurityError),
    #[error(transparent)]
    Store(#[from] ipfs_store::StoreError),
    #[error(transparent)]
    Unixfs(#[from] ipfs_unixfs::UnixfsError),
    #[error(transparent)]
    Cid(#[from] ipfs_cid::error::CidError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("network: {0}")]
    Network(String),
    #[error("not allowed for the {0} role")]
    Role(&'static str),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("ambiguous name, use the number: {0:?}")]
    Ambiguous(Vec<String>),
    #[error("rejected: {0}")]
    Rejected(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

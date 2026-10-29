use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid multiaddress '{value}': {reason}")]
    InvalidAddress { value: String, reason: String },

    #[error("identity error: {0}")]
    Identity(String),

    #[error("libp2p: {0}")]
    Libp2p(String),
}

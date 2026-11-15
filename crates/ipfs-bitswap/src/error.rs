use thiserror::Error;

#[derive(Debug, Error)]
pub enum BitswapError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid Bitswap message: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("Bitswap message too large ({actual} B > {limit} B)")]
    MessageTooLarge { actual: usize, limit: usize },

    #[error("blockstore: {0}")]
    Store(#[from] ipfs_store::StoreError),

    #[error("stream: {0}")]
    Stream(String),

    #[error("stream write timed out")]
    Timeout,
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnixfsError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("blockstore: {0}")]
    Store(#[from] ipfs_store::StoreError),

    #[error("CID: {0}")]
    Cid(#[from] ipfs_cid::error::CidError),

    #[error("invalid dag-pb or UnixFS block: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("invalid chunker parameters: {0}")]
    InvalidChunker(&'static str),

    #[error("block {0} is missing from the Blockstore")]
    MissingBlock(String),

    #[error("file declares {declared} B, limit is {limit} B")]
    FileTooLarge { declared: u64, limit: u64 },

    #[error("inconsistent DAG at {cid}: {reason}")]
    InvalidDag { cid: String, reason: &'static str },

    #[error("unsupported block {cid}: {reason}")]
    Unsupported { cid: String, reason: &'static str },
}

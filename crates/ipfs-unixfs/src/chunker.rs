//! Splits a file into fixed-size chunks (`size-N` in Kubo).
//! Rabin was removed: EHR documents are ciphertext, where content-defined boundaries give
//! no block reuse (reasoning: `docs/security-ucan-gpg-integration.md`, section 8).
use crate::consts::DEFAULT_CHUNK_BYTES;
use crate::error::UnixfsError;
use crate::models::{Chunker, Chunks};
use ipfs_store::consts::MAX_BLOCK_BYTES;
use std::io::{self, BufReader, Read};

impl Chunker {
    pub fn fixed() -> Self {
        Self::Fixed {
            size: DEFAULT_CHUNK_BYTES,
        }
    }

    /// Name in Kubo notation (`ipfs add --chunker=...`).
    pub fn kubo_name(&self) -> String {
        match self {
            Self::Fixed { size } => format!("size-{size}"),
        }
    }

    pub fn split<R: Read>(self, reader: R) -> Result<Chunks<R>, UnixfsError> {
        let Self::Fixed { size } = self;
        if size == 0 || size > MAX_BLOCK_BYTES {
            return Err(UnixfsError::InvalidChunker(
                "size must be between 1 and the block limit",
            ));
        }
        Ok(Chunks {
            reader: BufReader::new(reader),
            chunker: self,
        })
    }
}

impl<R: Read> Iterator for Chunks<R> {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let Chunker::Fixed { size } = self.chunker;
        let mut chunk = Vec::new();
        match (&mut self.reader).take(size as u64).read_to_end(&mut chunk) {
            Ok(0) => None,
            Ok(_) => Some(Ok(chunk)),
            Err(error) => Some(Err(error)),
        }
    }
}

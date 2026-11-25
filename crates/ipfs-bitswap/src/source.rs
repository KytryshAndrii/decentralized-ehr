use crate::error::BitswapError;
use crate::models::ProvidedBlockSource;
use ipfs_cid::models::Cid;
use ipfs_store::{PersistentBlockstore, StoreError};
use std::path::PathBuf;

/// Where the server takes blocks from; tests use an in-memory source.
pub trait BlockSource: Send + Sync + 'static {
    /// For each CID: whether the block may and can be served.
    fn has_blocks(&self, cids: &[Cid]) -> Result<Vec<bool>, BitswapError>;

    /// Block bytes, or `None` when the block is missing or not shared.
    fn get_block(&self, cid: &Cid) -> Result<Option<Vec<u8>>, BitswapError>;

    /// Several reads in one source session (e.g. one database open per message).
    fn with_reader<R>(
        &self,
        task: impl FnOnce(&ReadBlock<'_>) -> Result<R, BitswapError>,
    ) -> Result<R, BitswapError>
    where
        Self: Sized,
    {
        task(&|cid| self.get_block(cid))
    }
}

pub type ReadBlock<'a> = dyn Fn(&Cid) -> Result<Option<Vec<u8>>, BitswapError> + 'a;

impl ProvidedBlockSource {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    fn open(&self) -> Result<PersistentBlockstore, BitswapError> {
        Ok(PersistentBlockstore::open_in(&self.directory)?)
    }
}

/// A CID the Blockstore does not support is simply a missing block, not a failure.
fn unsupported_as_absent<T: Default>(result: Result<T, StoreError>) -> Result<T, BitswapError> {
    match result {
        Err(
            StoreError::UnsupportedCidVersion(_)
            | StoreError::UnsupportedHash(_)
            | StoreError::InvalidDigestLength { .. },
        ) => Ok(T::default()),
        other => Ok(other?),
    }
}

impl BlockSource for ProvidedBlockSource {
    fn has_blocks(&self, cids: &[Cid]) -> Result<Vec<bool>, BitswapError> {
        let store = self.open()?;
        cids.iter()
            .map(|cid| {
                Ok(unsupported_as_absent(store.has_provide(cid))?
                    && unsupported_as_absent(store.has(cid))?)
            })
            .collect()
    }

    fn get_block(&self, cid: &Cid) -> Result<Option<Vec<u8>>, BitswapError> {
        read_provided(&self.open()?, cid)
    }

    fn with_reader<R>(
        &self,
        task: impl FnOnce(&ReadBlock<'_>) -> Result<R, BitswapError>,
    ) -> Result<R, BitswapError> {
        let store = self.open()?;
        task(&|cid| read_provided(&store, cid))
    }
}

fn read_provided(store: &PersistentBlockstore, cid: &Cid) -> Result<Option<Vec<u8>>, BitswapError> {
    if !unsupported_as_absent(store.has_provide(cid))? {
        return Ok(None);
    }
    unsupported_as_absent(store.get_block(cid))
}

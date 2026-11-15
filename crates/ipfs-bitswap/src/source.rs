use crate::error::BitswapError;
use crate::models::ProvidedBlockSource;
use ipfs_cid::models::Cid;
use ipfs_store::{PersistentBlockstore, StoreError};
use std::path::PathBuf;

/// Skąd serwer bierze bloki; w testach podmieniane na źródło w pamięci.
pub trait BlockSource: Send + Sync + 'static {
    /// Dla każdego CID: czy blok wolno i da się wydać.
    fn has_blocks(&self, cids: &[Cid]) -> Result<Vec<bool>, BitswapError>;

    /// Bajty bloku albo `None`, gdy bloku nie ma lub nie jest udostępniany.
    fn get_block(&self, cid: &Cid) -> Result<Option<Vec<u8>>, BitswapError>;
}

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

/// CID, którego Blockstore nie obsługuje, to po prostu brak bloku, a nie awaria.
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
        let store = self.open()?;
        if !unsupported_as_absent(store.has_provide(cid))? {
            return Ok(None);
        }
        unsupported_as_absent(store.get_block(cid))
    }
}

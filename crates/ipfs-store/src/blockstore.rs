use crate::datastore::Datastore;
use crate::error::StoreError;
use crate::models::Blockstore;
use crate::utils::{get_multihash_key, validate_block_size, validate_cid};
use ipfs_cid::consts::{CODEC_RAW, SHA2_256};
use ipfs_cid::models::Cid;

impl<D: Datastore> Blockstore<D> {
    pub fn new_blockstore(datastore: D) -> Self {
        Self { datastore }
    }

    pub fn put(&self, bytes: &[u8]) -> Result<Cid, StoreError> {
        self.put_with_codec(bytes, CODEC_RAW, SHA2_256)
    }

    pub fn put_with_codec(
        &self,
        bytes: &[u8],
        codec: u64,
        hash_code: u64,
    ) -> Result<Cid, StoreError> {
        validate_block_size(bytes)?;
        let cid = Cid::generate_from_data(bytes, codec, hash_code)?;
        self.put_block(&cid, bytes)?;
        Ok(cid)
    }

    /// Zapisuje blok dostarczony przez inny moduł.
    /// Nie pozwala nadpisać uszkodzonego już istniejącego wpisu bez jawnej naprawy.
    pub fn put_block(&self, cid: &Cid, bytes: &[u8]) -> Result<(), StoreError> {
        validate_block_size(bytes)?;
        validate_cid(cid)?;

        if !cid.verify_block(bytes) {
            return Err(StoreError::IntegrityMismatch);
        }

        let key = get_multihash_key(&cid.multihash);

        if self.datastore.put_if_absent(&key, bytes)? {
            return Ok(());
        }

        // Klucz już istnieje: sprawdzamy, czy zawiera ten sam blok.
        match self.datastore.get(&key)? {
            Some(existing) if existing == bytes => Ok(()),
            Some(_) => Err(StoreError::CorruptStoredBlock),
            None => Err(StoreError::ConcurrentModification),
        }
    }

    /// Odczytuje blok. Błędy I/O i uszkodzenie danych nie są mylone z brakiem bloku.
    pub fn get_block(&self, cid: &Cid) -> Result<Option<Vec<u8>>, StoreError> {
        validate_cid(cid)?;

        let key = get_multihash_key(&cid.multihash);

        match self.datastore.get(&key)? {
            None => Ok(None),
            Some(bytes) => {
                validate_block_size(&bytes)?;

                if !cid.verify_block(&bytes) {
                    return Err(StoreError::IntegrityMismatch);
                }
                Ok(Some(bytes))
            }
        }
    }

    /// Sprawdza obecność klucza.
    pub fn has(&self, cid: &Cid) -> Result<bool, StoreError> {
        validate_cid(cid)?;
        self.datastore.has(&get_multihash_key(&cid.multihash))
    }

    pub fn datastore(&self) -> &D {
        &self.datastore
    }
}

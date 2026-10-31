use crate::consts::{DEFAULT_DB_PATH, LEVELDB_DIR_NAME};
use crate::datastore::Datastore;
use crate::error::StoreError;
use crate::models::{Blockstore, LevelDbDatastore};
use crate::utils::{get_multihash_key, validate_block_size, validate_cid};
use ipfs_cid::consts::{CODEC_RAW, SHA2_256};
use ipfs_cid::models::Cid;
use std::path::Path;

impl<D: Datastore> Blockstore<D> {
    pub fn new_blockstore(datastore: D) -> Self {
        Self { datastore }
    }

    /// Tworzy CIDv1 raw/SHA2-256 i zapisuje blok.
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

    /// Zapisuje blok po zweryfikowaniu CID. Deduplikuje po multihash.
    pub fn put_block(&self, cid: &Cid, bytes: &[u8]) -> Result<(), StoreError> {
        validate_block_size(bytes)?;
        validate_cid(cid)?;
        if !cid.verify_block(bytes) {
            return Err(StoreError::IntegrityMismatch);
        }

        let key = get_multihash_key(&cid.multihash);
        if self.datastore.put_if_absent(&key, bytes)? {
            eprintln!(
                "[ipfs-store] STORED cid={} bytes={}",
                cid.to_string_base32(),
                bytes.len()
            );
            return Ok(());
        }

        match self.datastore.get(&key)? {
            Some(existing) if existing == bytes => {
                eprintln!("[ipfs-store] ALREADY_EXISTS cid={}", cid.to_string_base32());
                Ok(())
            }
            Some(_) => Err(StoreError::CorruptStoredBlock),
            None => Err(StoreError::ConcurrentModification),
        }
    }

    /// Odczyt z kontrolą integralności; None oznacza brak bloku.
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

    /// Sprawdza tylko obecność klucza, bez ponownego haszowania bloku.
    pub fn has(&self, cid: &Cid) -> Result<bool, StoreError> {
        validate_cid(cid)?;
        self.datastore.has(&get_multihash_key(&cid.multihash))
    }

    pub fn datastore(&self) -> &D {
        &self.datastore
    }
}

impl Blockstore<LevelDbDatastore> {
    pub fn open_default() -> Result<Self, StoreError> {
        let datastore = LevelDbDatastore::open(DEFAULT_DB_PATH)?;
        Ok(Self::new_blockstore(datastore))
    }

    /// Dla testów integracyjnych. EHR-CLI używa open_default()
    pub fn open_in(base_directory: &Path) -> Result<Self, StoreError> {
        let datastore = LevelDbDatastore::open(base_directory.join(LEVELDB_DIR_NAME))?;
        Ok(Self::new_blockstore(datastore))
    }
}

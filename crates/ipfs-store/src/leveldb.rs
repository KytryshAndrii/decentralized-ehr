use crate::models::LevelDbDatastore;
use crate::{datastore::Datastore, error::StoreError};
use rusty_leveldb::{DB, Options};
use std::path::Path;
use std::sync::Mutex;

impl LevelDbDatastore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        std::fs::create_dir_all(path.as_ref())?;
        let options = Options {
            create_if_missing: true,
            paranoid_checks: true,
            ..Options::default()
        };
        let db = DB::open(path, options)?;
        Ok(Self { db: Mutex::new(db) })
    }

    /// Wymusza synchronizację wszystkich oczekujących operacji.
    pub fn flush(&self) -> Result<(), StoreError> {
        let mut db = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        db.flush()?;
        Ok(())
    }
}

impl Datastore for LevelDbDatastore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        let mut db = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        // `DB::get()` maskuje błędy jako None; używamy get_at(), który zwraca Result.
        let snapshot = db.get_snapshot();
        let value = db.get_at(&snapshot, key)?;
        Ok(value.map(|bytes| bytes.to_vec()))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        let mut db = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        db.put(key, value)?;
        db.flush()?;
        Ok(())
    }

    fn put_if_absent(&self, key: &[u8], value: &[u8]) -> Result<bool, StoreError> {
        let mut db = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;

        if db.get(key).is_some() {
            return Ok(false);
        }

        db.put(key, value)?;
        db.flush()?;

        Ok(true)
    }
}

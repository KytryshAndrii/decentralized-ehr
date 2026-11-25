use crate::consts::{LOCK_RETRY_ATTEMPTS, LOCK_RETRY_DELAY_MS};
use crate::datastore::{Datastore, Entry};
use crate::error::StoreError;
use crate::models::LevelDbDatastore;
use rusty_leveldb::{DB, LdbIterator, Options, StatusCode};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

impl LevelDbDatastore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        std::fs::create_dir_all(path.as_ref())?;
        let options = Options {
            create_if_missing: true,
            paranoid_checks: true,
            ..Options::default()
        };
        let database = Self::open_with_lock_retry(path.as_ref(), options)?;
        Ok(Self {
            db: Mutex::new(database),
        })
    }

    /// The database is opened only for one operation, so the LOCK file held by another node
    /// thread or by `ehr-cli put` in a second terminal is released quickly.
    fn open_with_lock_retry(path: &Path, options: Options) -> Result<DB, StoreError> {
        let mut attempt = 0;
        loop {
            match DB::open(path, options.clone()) {
                Err(status)
                    if status.code == StatusCode::LockError && attempt < LOCK_RETRY_ATTEMPTS =>
                {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(LOCK_RETRY_DELAY_MS));
                }
                result => return Ok(result?),
            }
        }
    }

    pub fn flush(&self) -> Result<(), StoreError> {
        let mut database = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        database.flush()?;
        Ok(())
    }

    fn scan_prefix(
        &self,
        prefix: &[u8],
        mut visit: impl FnMut(&[u8], &[u8]),
    ) -> Result<(), StoreError> {
        let mut database = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        let mut iterator = database.new_iter()?;
        // Keys are sorted, so entries sharing a prefix are adjacent.
        iterator.seek(prefix);
        while iterator.valid() {
            let Some((key, value)) = iterator.current() else {
                break;
            };
            if !key.starts_with(prefix) {
                break;
            }
            visit(&key, &value);
            if !iterator.advance() {
                break;
            }
        }
        Ok(())
    }
}

impl Datastore for LevelDbDatastore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        let mut database = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        // `DB::get()` hides errors as None; get_at() returns a Result.
        let snapshot = database.get_snapshot();
        let value = database.get_at(&snapshot, key)?;
        Ok(value.map(|bytes| bytes.to_vec()))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        let mut database = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        database.put(key, value)?;
        database.flush()?;
        Ok(())
    }

    fn put_if_absent(&self, key: &[u8], value: &[u8]) -> Result<bool, StoreError> {
        let mut database = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;

        if database.get(key).is_some() {
            return Ok(false);
        }

        database.put(key, value)?;
        database.flush()?;

        Ok(true)
    }

    fn delete(&self, key: &[u8]) -> Result<(), StoreError> {
        let mut database = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        database.delete(key)?;
        database.flush()?;
        Ok(())
    }

    fn entries_with_prefix(&self, prefix: &[u8]) -> Result<Vec<Entry>, StoreError> {
        let mut entries = Vec::new();
        self.scan_prefix(prefix, |key, value| {
            entries.push((key.to_vec(), value.to_vec()))
        })?;
        Ok(entries)
    }

    fn key_sizes_with_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, usize)>, StoreError> {
        let mut sizes = Vec::new();
        self.scan_prefix(prefix, |key, value| sizes.push((key.to_vec(), value.len())))?;
        Ok(sizes)
    }
}

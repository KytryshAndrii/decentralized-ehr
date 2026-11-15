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
        let db = Self::open_with_lock_retry(path.as_ref(), options)?;
        Ok(Self { db: Mutex::new(db) })
    }

    /// Baza jest otwierana tylko na czas jednej operacji, więc blokada pliku LOCK
    /// (inny wątek węzła albo `ehr-cli put` w drugim terminalu) zwalnia się szybko.
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

    fn delete(&self, key: &[u8]) -> Result<(), StoreError> {
        let mut db = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        db.delete(key)?;
        db.flush()?;
        Ok(())
    }

    fn entries_with_prefix(&self, prefix: &[u8]) -> Result<Vec<Entry>, StoreError> {
        let mut db = self.db.lock().map_err(|_| StoreError::LockPoisoned)?;
        let mut iterator = db.new_iter()?;
        // Klucze są posortowane, więc wpisy z jednym prefiksem leżą obok siebie.
        iterator.seek(prefix);
        let mut entries = Vec::new();
        while iterator.valid() {
            let Some((key, value)) = iterator.current() else {
                break;
            };
            if !key.starts_with(prefix) {
                break;
            }
            entries.push((key.to_vec(), value.to_vec()));
            if !iterator.advance() {
                break;
            }
        }
        Ok(entries)
    }
}

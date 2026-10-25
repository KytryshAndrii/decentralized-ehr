use ipfs_cid::consts::{CODEC_RAW, SHA2_256, SHA2_512};
use ipfs_cid::models::Cid;
use ipfs_store::{Blockstore, Datastore, StoreError};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Default)]
struct MemoryDatastore {
    entries: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
}

impl Datastore for MemoryDatastore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        self.entries
            .lock()
            .unwrap()
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    fn put_if_absent(&self, key: &[u8], value: &[u8]) -> Result<bool, StoreError> {
        let mut map = self.entries.lock().unwrap();
        if map.contains_key(key) {
            return Ok(false);
        }
        map.insert(key.to_vec(), value.to_vec());
        Ok(true)
    }
}

struct SharedDatastore(Arc<MemoryDatastore>);
impl Datastore for SharedDatastore {
    fn get(&self, k: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.0.get(k)
    }
    fn put(&self, k: &[u8], v: &[u8]) -> Result<(), StoreError> {
        self.0.put(k, v)
    }
    fn put_if_absent(&self, k: &[u8], v: &[u8]) -> Result<bool, StoreError> {
        self.0.put_if_absent(k, v)
    }
}

fn cid(data: &[u8]) -> Cid {
    Cid::generate_from_data(data, CODEC_RAW, SHA2_256).unwrap()
}

#[test]
fn empty_block_is_valid() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let id = store.put(b"").unwrap();
    assert_eq!(store.get_block(&id).unwrap().unwrap(), b"");
}
#[test]
fn one_byte_and_hi_are_valid() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    for input in [&b"x"[..], &b"Hi!"[..]] {
        let id = store.put(input).unwrap();
        assert_eq!(store.get_block(&id).unwrap().unwrap(), input);
        assert!(store.has(&id).unwrap());
    }
}
#[test]
fn exactly_max_size_accepted_and_one_more_rejected() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let max = ipfs_store::consts::MAX_BLOCK_BYTES;
    assert!(store.put(&vec![42; max]).is_ok());
    assert!(
        matches!(store.put(&vec![42; max+1]), Err(StoreError::BlockTooLarge{actual,limit}) if actual==max+1 && limit==max)
    );
}
#[test]
fn missing_block_is_not_an_error() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let id = cid(b"absent");
    assert!(!store.has(&id).unwrap());
    assert!(store.get_block(&id).unwrap().is_none());
}
#[test]
fn put_is_idempotent() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let a = store.put(b"Hi!").unwrap();
    let b = store.put(b"Hi!").unwrap();
    assert_eq!(a, b);
    assert_eq!(store.datastore().entries.lock().unwrap().len(), 1);
}
#[test]
fn same_content_with_different_codec_is_deduplicated() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let a = store.put_with_codec(b"Hi!", CODEC_RAW, SHA2_256).unwrap();
    let b = store.put_with_codec(b"Hi!", 0x70, SHA2_256).unwrap();
    assert_ne!(a.codec, b.codec);
    assert_eq!(store.datastore().entries.lock().unwrap().len(), 1);
    assert_eq!(store.get_block(&b).unwrap().unwrap(), b"Hi!");
}
#[test]
fn different_hash_algorithms_have_different_keys() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let a = store.put_with_codec(b"Hi!", CODEC_RAW, SHA2_256).unwrap();
    let b = store.put_with_codec(b"Hi!", CODEC_RAW, SHA2_512).unwrap();
    assert_ne!(a.multihash.digest.len(), b.multihash.digest.len());
    assert_eq!(store.datastore().entries.lock().unwrap().len(), 2);
}
#[test]
fn rejecting_bytes_that_do_not_match_cid() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let id = cid(b"original");
    assert!(matches!(
        store.put_block(&id, b"tampered"),
        Err(StoreError::IntegrityMismatch)
    ));
    assert!(!store.has(&id).unwrap());
}
#[test]
fn rejecting_invalid_version_and_digest_length() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let mut id = cid(b"Hi!");
    id.version = 2;
    assert!(matches!(
        store.has(&id),
        Err(StoreError::UnsupportedCidVersion(2))
    ));
    id.version = 1;
    id.multihash.digest.pop();
    assert!(matches!(
        store.has(&id),
        Err(StoreError::InvalidDigestLength { .. })
    ));
}
#[test]
fn rejecting_unsupported_algorithm() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let mut id = cid(b"Hi!");
    id.multihash.code = 0x777;
    assert!(matches!(
        store.has(&id),
        Err(StoreError::UnsupportedHash(0x777))
    ));
}
#[test]
fn corrupted_stored_bytes_are_detected_on_get() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let id = store.put(b"Hi!").unwrap();
    let key = ipfs_store::utils::get_multihash_key(&id.multihash);
    store.datastore().put(&key, b"bad").unwrap();
    assert!(store.has(&id).unwrap()); // has nie haszuje
    assert!(matches!(
        store.get_block(&id),
        Err(StoreError::IntegrityMismatch)
    ));
    assert!(matches!(
        store.put_block(&id, b"Hi!"),
        Err(StoreError::CorruptStoredBlock)
    ));
}
#[test]
fn oversized_stored_block_is_rejected() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let id = cid(b"Hi!");
    let key = ipfs_store::utils::get_multihash_key(&id.multihash);
    store
        .datastore()
        .put(&key, &vec![0; ipfs_store::consts::MAX_BLOCK_BYTES + 1])
        .unwrap();
    assert!(matches!(
        store.get_block(&id),
        Err(StoreError::BlockTooLarge { .. })
    ));
}
#[test]
fn key_is_namespace_plus_multihash_not_codec() {
    let id = cid(b"Hi!");
    let key = ipfs_store::utils::get_multihash_key(&id.multihash);
    assert!(key.starts_with(b"/blocks/"));
    assert_eq!(&key[8..10], &[0x12, 0x20]);
    assert_eq!(&key[10..], id.multihash.digest.as_slice());
    assert_eq!(key.len(), 42);
}
#[test]
fn concurrent_writers_of_identical_content_are_idempotent() {
    let data = Arc::new(MemoryDatastore::default());
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let storage = SharedDatastore(Arc::clone(&data));
            thread::spawn(move || Blockstore::new_blockstore(storage).put(b"Hi!"))
        })
        .collect();
    for handle in handles {
        assert!(handle.join().unwrap().is_ok());
    }
    assert_eq!(data.entries.lock().unwrap().len(), 1);
}

#[derive(Default)]
struct FailingDatastore;
impl Datastore for FailingDatastore {
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Err(StoreError::LockPoisoned)
    }
    fn put(&self, _: &[u8], _: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::LockPoisoned)
    }
    fn put_if_absent(&self, _: &[u8], _: &[u8]) -> Result<bool, StoreError> {
        Err(StoreError::LockPoisoned)
    }
}
#[test]
fn datastore_errors_are_propagated() {
    let store = Blockstore::new_blockstore(FailingDatastore);
    let id = cid(b"Hi!");
    assert!(matches!(store.has(&id), Err(StoreError::LockPoisoned)));
    assert!(matches!(
        store.get_block(&id),
        Err(StoreError::LockPoisoned)
    ));
    assert!(matches!(
        store.put_block(&id, b"Hi!"),
        Err(StoreError::LockPoisoned)
    ));
}

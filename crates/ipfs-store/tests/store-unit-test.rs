use ipfs_cid::consts::{CODEC_RAW, SHA2_256, SHA2_512};
use ipfs_cid::models::Cid;
use ipfs_store::datastore::Entry;
use ipfs_store::{Blockstore, Datastore, KnownPeer, Namespace, RemoteProvider, StoreError};
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
    fn delete(&self, key: &[u8]) -> Result<(), StoreError> {
        self.entries.lock().unwrap().remove(key);
        Ok(())
    }
    fn entries_with_prefix(&self, prefix: &[u8]) -> Result<Vec<Entry>, StoreError> {
        let map = self.entries.lock().unwrap();
        let mut entries: Vec<Entry> = map
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        entries.sort();
        Ok(entries)
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
    fn delete(&self, k: &[u8]) -> Result<(), StoreError> {
        self.0.delete(k)
    }
    fn entries_with_prefix(&self, p: &[u8]) -> Result<Vec<Entry>, StoreError> {
        self.0.entries_with_prefix(p)
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
    fn delete(&self, _: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::LockPoisoned)
    }
    fn entries_with_prefix(&self, _: &[u8]) -> Result<Vec<Entry>, StoreError> {
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

#[test]
fn namespace_prefixes_match_consts_and_are_disjoint() {
    use ipfs_store::consts::{BLOCKS_PREFIX, CACHE_PREFIX, PEERS_PREFIX, PINS_PREFIX};
    assert_eq!(Namespace::Blocks.prefix(), BLOCKS_PREFIX.as_bytes());
    assert_eq!(Namespace::Peers.prefix(), PEERS_PREFIX.as_bytes());
    assert_eq!(Namespace::Pins.prefix(), PINS_PREFIX.as_bytes());
    assert_eq!(Namespace::Cache.prefix(), CACHE_PREFIX.as_bytes());
    assert_eq!(
        Namespace::RemoteProviders.prefix(),
        ipfs_store::consts::REMOTE_PROVIDERS_PREFIX.as_bytes()
    );
    assert_eq!(
        Namespace::Provides.prefix(),
        ipfs_store::consts::PROVIDES_PREFIX.as_bytes()
    );
    for a in Namespace::ALL {
        for b in Namespace::ALL {
            let key = a.key(b"id");
            assert_eq!(b.owns(&key), a == b, "{a:?} vs {b:?}");
        }
    }
}
#[test]
fn block_key_layout_is_unchanged() {
    let id = cid(b"Hi!");
    let key = ipfs_store::utils::get_multihash_key(&id.multihash);
    let mut expected = b"/blocks/".to_vec();
    expected.extend_from_slice(&[0x12, 0x20]);
    expected.extend_from_slice(&id.multihash.digest);
    assert_eq!(key, expected);
}
#[test]
fn same_id_in_other_namespace_does_not_collide_with_block() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let id = store.put(b"Hi!").unwrap();
    let pin_key = ipfs_store::utils::get_namespaced_multihash_key(Namespace::Pins, &id.multihash);
    assert!(!store.datastore().has(&pin_key).unwrap());
    store.datastore().put(&pin_key, b"direct").unwrap();
    assert_eq!(store.get_block(&id).unwrap().unwrap(), b"Hi!");
    assert_eq!(store.datastore().get(&pin_key).unwrap().unwrap(), b"direct");
}

fn known_peer(id: &str, server: bool) -> KnownPeer {
    KnownPeer {
        peer_id: id.into(),
        addresses: vec!["/ip4/192.168.1.125/tcp/4101".into()],
        dht_server: server,
        last_seen_utc: 1_700_000_000,
    }
}
#[test]
fn peers_are_saved_listed_overwritten_and_removed() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    assert!(store.load_peers().unwrap().is_empty());
    store.save_peer(&known_peer("12D3KooWB", true)).unwrap();
    store.save_peer(&known_peer("12D3KooWA", false)).unwrap();
    assert_eq!(
        store.load_peers().unwrap(),
        vec![
            known_peer("12D3KooWA", false),
            known_peer("12D3KooWB", true)
        ]
    );
    let mut updated = known_peer("12D3KooWA", true);
    updated.addresses.push("/ip4/10.0.0.1/tcp/4101".into());
    store.save_peer(&updated).unwrap();
    assert_eq!(store.load_peers().unwrap()[0], updated);
    store.remove_peer("12D3KooWA").unwrap();
    store.remove_peer("12D3KooWA").unwrap();
    assert_eq!(
        store.load_peers().unwrap(),
        vec![known_peer("12D3KooWB", true)]
    );
}
#[test]
fn stale_peers_are_pruned_and_old_records_count_as_seen_now() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let mut old = known_peer("12D3KooWA", true);
    old.last_seen_utc = 100;
    let mut fresh = known_peer("12D3KooWB", false);
    fresh.last_seen_utc = 200;
    store.save_peer(&old).unwrap();
    store.save_peer(&fresh).unwrap();
    // Rekord w starym formacie, bez linii `seen_utc=`.
    store
        .datastore()
        .put(
            &ipfs_store::utils::get_peer_key("12D3KooWC"),
            b"dht=server\n/ip4/192.168.1.7/tcp/4101",
        )
        .unwrap();
    let (kept, removed) = store.prune_peers(150).unwrap();
    assert_eq!(removed, 1);
    let ids: Vec<_> = kept.iter().map(|peer| peer.peer_id.as_str()).collect();
    assert_eq!(ids, ["12D3KooWB", "12D3KooWC"]);
    assert_eq!(kept[1].addresses, ["/ip4/192.168.1.7/tcp/4101"]);
    assert_eq!(store.load_peers().unwrap().len(), 2);
}
#[test]
fn invalid_or_corrupt_peer_is_rejected() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    assert!(matches!(
        store.save_peer(&known_peer("", true)),
        Err(StoreError::InvalidPeer(_))
    ));
    let mut bad_address = known_peer("12D3KooWA", true);
    bad_address.addresses.push("two\nlines".into());
    assert!(matches!(
        store.save_peer(&bad_address),
        Err(StoreError::InvalidPeer(_))
    ));
    store
        .datastore()
        .put(&ipfs_store::utils::get_peer_key("12D3KooWA"), b"garbage")
        .unwrap();
    assert!(matches!(
        store.load_peers(),
        Err(StoreError::CorruptRecord(_))
    ));
}
#[test]
fn provides_are_tracked_separately_from_blocks() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let first = store.put(b"first").unwrap();
    let second = store.put(b"second").unwrap();
    assert!(store.list_provides().unwrap().is_empty());
    assert!(!store.has_provide(&first).unwrap());
    store.add_provide(&first).unwrap();
    store.add_provide(&first).unwrap();
    assert!(store.has_provide(&first).unwrap());
    assert!(!store.has_provide(&second).unwrap());
    store.add_provide(&second).unwrap();
    let mut listed = store.list_provides().unwrap();
    listed.sort_by_key(|cid| cid.to_string_base32());
    let mut expected = vec![first.clone(), second.clone()];
    expected.sort_by_key(|cid| cid.to_string_base32());
    assert_eq!(listed.len(), 2);
    for (a, b) in listed.iter().zip(&expected) {
        assert_eq!(a.to_bytes(), b.to_bytes());
    }
    store.remove_provide(&first).unwrap();
    assert!(!store.has_provide(&first).unwrap());
    let listed = store.list_provides().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].to_bytes(), second.to_bytes());
    assert!(store.has(&first).unwrap());
    assert!(store.load_peers().unwrap().is_empty());
}

fn remote_provider(data: &[u8], peer: &str, expires_at_utc: u64) -> RemoteProvider {
    RemoteProvider {
        multihash: ipfs_store::utils::multihash_bytes(&cid(data).multihash),
        peer_id: peer.into(),
        addresses: vec!["/ip4/192.168.1.51/tcp/4101".into()],
        expires_at_utc,
    }
}
#[test]
fn remote_providers_are_saved_refreshed_and_expire_by_utc_time() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let now = 1_800_000_000;
    store
        .save_remote_provider(&remote_provider(b"x", "12D3KooWA", now + 10))
        .unwrap();
    store
        .save_remote_provider(&remote_provider(b"x", "12D3KooWB", now + 100))
        .unwrap();
    store
        .save_remote_provider(&remote_provider(b"y", "12D3KooWA", now + 100))
        .unwrap();
    assert_eq!(store.load_remote_providers(now).unwrap().len(), 3);
    // Ponowne ogłoszenie odświeża termin tego samego rekordu.
    store
        .save_remote_provider(&remote_provider(b"x", "12D3KooWA", now + 500))
        .unwrap();
    let loaded = store.load_remote_providers(now + 10).unwrap();
    assert_eq!(loaded.len(), 3);
    assert!(loaded.contains(&remote_provider(b"x", "12D3KooWA", now + 500)));
    // Po terminie rekord nie jest zwracany, a porządkowanie usuwa go z bazy.
    assert_eq!(store.load_remote_providers(now + 100).unwrap().len(), 1);
    let (valid, removed) = store.prune_remote_providers(now + 100).unwrap();
    assert_eq!((valid.len(), removed), (1, 2));
    assert_eq!(store.prune_remote_providers(now + 100).unwrap().1, 0);
    assert_eq!(
        store.load_remote_providers(0).unwrap(),
        vec![remote_provider(b"x", "12D3KooWA", now + 500)]
    );
    assert!(store.load_peers().unwrap().is_empty());
    assert!(store.list_provides().unwrap().is_empty());
}
#[test]
fn invalid_or_corrupt_remote_provider_is_rejected() {
    let store = Blockstore::new_blockstore(MemoryDatastore::default());
    let mut bad_hash = remote_provider(b"x", "12D3KooWA", 10);
    bad_hash.multihash.pop();
    assert!(matches!(
        store.save_remote_provider(&bad_hash),
        Err(StoreError::InvalidProvider(_))
    ));
    assert!(matches!(
        store.save_remote_provider(&remote_provider(b"x", "", 10)),
        Err(StoreError::InvalidProvider(_))
    ));
    let valid = remote_provider(b"x", "12D3KooWA", 10);
    let key = ipfs_store::utils::get_remote_provider_key(&valid.multihash, &valid.peer_id);
    store.datastore().put(&key, b"no expiry line").unwrap();
    assert!(matches!(
        store.load_remote_providers(0),
        Err(StoreError::CorruptRecord(_))
    ));
}
#[test]
fn utc_clock_returns_unix_seconds() {
    // 2026-01-01T00:00:00Z = 1_767_225_600
    assert!(ipfs_store::utils::utc_now_secs() > 1_767_225_600);
}

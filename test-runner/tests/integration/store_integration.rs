use ipfs_store::{
    Datastore, PersistentBlockstore, StoreError, consts::LEVELDB_DIR_NAME, models::LevelDbDatastore,
};
use std::{sync::Arc, thread};
use tempfile::tempdir;

fn isolated_store(root: &std::path::Path) -> PersistentBlockstore {
    PersistentBlockstore::open_in(root).unwrap()
}

#[test]
fn fixed_directory_name_is_used_in_tests() {
    let root = tempdir().unwrap();
    {
        let store = isolated_store(root.path());
        store.put(b"Hi!").unwrap();
    }
    assert!(root.path().join(LEVELDB_DIR_NAME).is_dir());
}

#[test]
fn persists_data_across_close_and_reopen() {
    let root = tempdir().unwrap();
    let cid = {
        let store = isolated_store(root.path());
        store.put(b"Hi!").unwrap()
    };
    let store = isolated_store(root.path());
    assert!(store.has(&cid).unwrap());
    assert_eq!(store.get_block(&cid).unwrap().unwrap(), b"Hi!");
}

#[test]
fn database_backend_rejects_corrupted_existing_entry() {
    let root = tempdir().unwrap();
    let store = isolated_store(root.path());
    let cid = store.put(b"Hi!").unwrap();
    let key = ipfs_store::utils::get_multihash_key(&cid.multihash);
    // Celowe obejście walidacji tylko w teście awarii backendu.
    store.datastore().put(&key, b"corrupted").unwrap();
    assert!(matches!(
        store.get_block(&cid),
        Err(StoreError::IntegrityMismatch)
    ));
}

#[test]
fn leveldb_put_if_absent_is_atomic_for_shared_handle() {
    let root = tempdir().unwrap();
    let store = Arc::new(isolated_store(root.path()));
    let handles: Vec<_> = (0..12)
        .map(|_| {
            let store = Arc::clone(&store);
            thread::spawn(move || store.put(b"Hi!"))
        })
        .collect();
    let ids: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().unwrap().unwrap())
        .collect();
    assert!(ids.iter().all(|cid| cid == &ids[0]));
    assert_eq!(store.get_block(&ids[0]).unwrap().unwrap(), b"Hi!");
}

#[test]
fn leveldb_explicit_flush_keeps_data() {
    let root = tempdir().unwrap();
    let cid = {
        let store = isolated_store(root.path());
        let cid = store.put(b"persist").unwrap();
        store.datastore().flush().unwrap();
        cid
    };
    assert_eq!(
        isolated_store(root.path())
            .get_block(&cid)
            .unwrap()
            .unwrap(),
        b"persist"
    );
}

#[test]
fn fixture_bytes_roundtrip_without_parsing() {
    let root = tempdir().unwrap();
    let store = isolated_store(root.path());
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("mocks");
    fn visit(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    visit(&p, files);
                } else if p.is_file() {
                    files.push(p);
                }
            }
        }
    }
    let mut files = Vec::new();
    visit(&fixture_root, &mut files);
    let mut checked = 0;
    for path in files.into_iter().filter(|p| {
        matches!(
            p.extension().and_then(|e| e.to_str()),
            Some(
                "json"
                    | "xml"
                    | "ttl"
                    | "jpg"
                    | "jpeg"
                    | "png"
                    | "webp"
                    | "bmp"
                    | "tiff"
                    | "svg"
                    | "txt"
            )
        )
    }) {
        let bytes = std::fs::read(&path).unwrap();
        if bytes.len() > ipfs_store::consts::MAX_BLOCK_BYTES {
            continue;
        }
        let cid = store.put(&bytes).unwrap();
        assert_eq!(
            store.get_block(&cid).unwrap().unwrap(),
            bytes,
            "fixture {path:?}"
        );
        checked += 1;
    }
    if checked == 0 {
        let bytes = br#"{"resourceType":"Patient","id":"synthetic"}"#;
        let cid = store.put(bytes).unwrap();
        assert_eq!(store.get_block(&cid).unwrap().unwrap(), bytes);
    }
}

#[test]
fn same_leveldb_directory_cannot_be_opened_twice() {
    let root = tempdir().unwrap();
    let db_path = root.path().join(LEVELDB_DIR_NAME);
    let first = LevelDbDatastore::open(&db_path).unwrap();
    let second = LevelDbDatastore::open(&db_path);
    assert!(second.is_err());
    drop(first);
    assert!(LevelDbDatastore::open(&db_path).is_ok());
}

#[test]
fn default_path_constant_is_in_sync_with_database_name() {
    assert!(ipfs_store::consts::DEFAULT_DB_PATH.ends_with(LEVELDB_DIR_NAME));
}

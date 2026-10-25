use ipfs_store::{Blockstore, Datastore, LevelDbDatastore, StoreError};
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

fn store_at(path: &std::path::Path) -> Blockstore<LevelDbDatastore> {
    Blockstore::new_blockstore(LevelDbDatastore::open(path).unwrap())
}

#[test]
fn persists_data_across_close_and_reopen() {
    let temp = tempdir().unwrap();
    let id = {
        let store = store_at(temp.path());
        store.put(b"Hi!").unwrap()
    };
    let store = store_at(temp.path());
    assert!(store.has(&id).unwrap());
    assert_eq!(store.get_block(&id).unwrap().unwrap(), b"Hi!");
}

#[test]
fn database_backend_rejects_corrupted_existing_entry() {
    let temp = tempdir().unwrap();
    let store = store_at(temp.path());
    let id = store.put(b"Hi!").unwrap();
    let key = {
        let mut key = b"/blocks/".to_vec();
        key.extend_from_slice(&[0x12, 0x20]);
        key.extend_from_slice(&id.multihash.digest);
        key
    };
    // Celowe obejście Blockstore: wyłącznie do testu korupcji backendu.
    store.datastore().put(&key, b"corrupted").unwrap();
    assert!(matches!(
        store.get_block(&id),
        Err(StoreError::IntegrityMismatch)
    ));
}
#[test]
fn database_leveldb_put_if_absent_is_atomic_for_shared_handle() {
    let temp = tempdir().unwrap();
    let store = Arc::new(store_at(temp.path()));
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
    for id in &ids {
        assert_eq!(id, &ids[0]);
    }
    assert_eq!(store.get_block(&ids[0]).unwrap().unwrap(), b"Hi!");
}
#[test]
fn leveldb_explicit_flush_keeps_data() {
    let temp = tempdir().unwrap();
    let id = {
        let store = store_at(temp.path());
        let id = store.put(b"persist").unwrap();
        store.datastore().flush().unwrap();
        id
    };
    assert_eq!(
        store_at(temp.path()).get_block(&id).unwrap().unwrap(),
        b"persist"
    );
}
#[test]
fn fixture_bytes_roundtrip_without_parsing() {
    let temp = tempdir().unwrap();
    let store = store_at(temp.path());
    // Spróbuj prawdziwych fixtures repo, a w ostateczności użyj lokalnej atrapy.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mocks = root.join("tests/mocks");
    let mut count = 0;
    fn visit(path: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        if let Ok(items) = std::fs::read_dir(path) {
            for entry in items.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    visit(&p, files)
                } else if p.is_file() {
                    files.push(p)
                }
            }
        }
    }
    let mut files = Vec::new();
    visit(&mocks, &mut files);
    for file in files.into_iter().filter(|p| {
        matches!(
            p.extension().and_then(|x| x.to_str()).unwrap_or(""),
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
    }) {
        let bytes = std::fs::read(&file).unwrap();
        if bytes.len() > 2 * 1024 * 1024 {
            continue;
        }
        let cid = store.put(&bytes).unwrap();
        assert_eq!(
            store.get_block(&cid).unwrap().unwrap(),
            bytes,
            "fixture {:?}",
            file
        );
        count += 1;
    }
    if count == 0 {
        let bytes = br#"{"resourceType":"Patient","id":"synthetic"}"#;
        let cid = store.put(bytes).unwrap();
        assert_eq!(store.get_block(&cid).unwrap().unwrap(), bytes);
    }
}
#[test]
fn cannot_open_same_leveldb_directory_twice_at_same_time() {
    let temp = tempdir().unwrap();
    let first = LevelDbDatastore::open(temp.path()).unwrap();
    // Zachowanie locka katalogu zależy od backendu/systemu.
    let second = LevelDbDatastore::open(temp.path());
    assert!(
        second.is_err(),
        "LevelDB should lock an already-open database directory"
    );
    drop(first);
    assert!(LevelDbDatastore::open(temp.path()).is_ok());
}

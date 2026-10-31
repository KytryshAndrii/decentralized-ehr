use crate::datastore::Datastore;
use rusty_leveldb::DB;
use std::sync::Mutex;

/// Generyczny Blockstore: w testach można użyć MemoryDatastore.
pub struct Blockstore<D: Datastore> {
    pub(crate) datastore: D,
}

/// Backend infrastrukturalny używany wewnętrznie przez ipfs-store.
pub struct LevelDbDatastore {
    pub(crate) db: Mutex<DB>,
}

/// Publiczna fasada dla trwałego Blockstore.
pub type PersistentBlockstore = Blockstore<LevelDbDatastore>;

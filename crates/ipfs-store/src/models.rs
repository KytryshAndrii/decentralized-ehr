use crate::datastore::Datastore;
use rusty_leveldb::DB;
use std::sync::Mutex;
pub struct Blockstore<D: Datastore> {
    pub(crate) datastore: D,
}

/// Backend LevelDB. Jeden uchwyt DB może być współdzielony pomiędzy wątkami
/// przez Mutex; katalogu nie wolno równolegle otwierać drugim procesem.
pub struct LevelDbDatastore {
    pub db: Mutex<DB>,
}

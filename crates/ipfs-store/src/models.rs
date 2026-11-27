use crate::datastore::Datastore;
use rusty_leveldb::DB;
use std::sync::Mutex;

/// Generic Blockstore: tests can use an in-memory datastore.
pub struct Blockstore<D: Datastore> {
    pub(crate) datastore: D,
}

/// LevelDB backend used inside ipfs-store.
pub struct LevelDbDatastore {
    pub(crate) db: Mutex<DB>,
}

/// Public facade of the persistent Blockstore.
pub type PersistentBlockstore = Blockstore<LevelDbDatastore>;

/// Logical namespace ("folder") in the key-value database.
/// A new kind of data = a new variant + a prefix constant in `consts.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// Block bytes, key: multihash.
    Blocks,
    /// Known peers, key: PeerId.
    Peers,
    /// Pins (blocks kept by GC), key: multihash.
    Pins,
    /// Cache metadata (unpinned blocks), key: multihash; reserved for later.
    Cache,
    /// CIDs this node announces in the DHT, key: multihash.
    Provides,
    /// DHT records "peer P has block X" received from other nodes, key: multihash + PeerId.
    RemoteProviders,
}

/// Provider record from the network: "peer `peer_id` has the block with this multihash".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteProvider {
    pub multihash: Vec<u8>,
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub expires_at_utc: u64,
}

/// Peer remembered between node restarts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownPeer {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub dht_server: bool,
    pub last_seen_utc: u64,
}

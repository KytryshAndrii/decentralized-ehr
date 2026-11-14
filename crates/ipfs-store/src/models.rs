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

/// Logiczna przestrzeń nazw ("folder") w bazie key-value.
/// Nowy rodzaj danych = nowy wariant + stała prefiksu w `consts.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// Bajty bloków, klucz: multihash.
    Blocks,
    /// Znane peery, klucz: PeerId.
    Peers,
    /// Indeks pinów, klucz: multihash.
    Pins,
    /// Metadane cache (bloki nieprzypięte), klucz: multihash.
    Cache,
    /// CID ogłaszane w DHT przez ten węzeł, klucz: multihash.
    Provides,
    /// Rekordy DHT „peer P ma blok X” otrzymane od innych węzłów, klucz: multihash + PeerId.
    RemoteProviders,
}

/// Rekord providera otrzymany z sieci: „peer `peer_id` ma blok o tym multihashu”.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteProvider {
    pub multihash: Vec<u8>,
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub expires_at_utc: u64,
}

/// Peer zapamiętany między restartami węzła.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownPeer {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub dht_server: bool,
}

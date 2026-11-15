/// Limit pojedynczego bloku w prototypie.
pub const MAX_BLOCK_BYTES: usize = 2 * 1024 * 1024;

pub const LEVELDB_DIR_NAME: &str = "ehr-blockstore.leveldb";

/// Katalog danych węzła; `open_in(DEFAULT_DATA_DIR)` daje tę samą bazę co `open_default()`.
pub const DEFAULT_DATA_DIR: &str = "data";

pub const DEFAULT_DB_PATH: &str = "data/ehr-blockstore.leveldb";

/// Prefiksy przestrzeni nazw w LevelDB
pub const BLOCKS_PREFIX: &str = "/blocks/";
pub const PEERS_PREFIX: &str = "/peers/";
pub const PINS_PREFIX: &str = "/pins/";
pub const CACHE_PREFIX: &str = "/cache/";
pub const PROVIDES_PREFIX: &str = "/provides/";
pub const REMOTE_PROVIDERS_PREFIX: &str = "/remote-providers/";

pub const PEER_DHT_SERVER_MARKER: &str = "dht=server";
pub const PEER_DHT_CLIENT_MARKER: &str = "dht=client";
pub const PEER_SEEN_FIELD: &str = "seen_utc=";

pub const PROVIDER_EXPIRES_FIELD: &str = "expires_utc=";

/// Ponawianie otwarcia bazy, gdy plik LOCK trzyma inna, krótka operacja.
pub const LOCK_RETRY_ATTEMPTS: u32 = 40;
pub const LOCK_RETRY_DELAY_MS: u64 = 25;

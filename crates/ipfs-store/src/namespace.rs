use crate::consts::{
    BLOCKS_PREFIX, CACHE_PREFIX, PEERS_PREFIX, PINS_PREFIX, PROVIDES_PREFIX,
    REMOTE_PROVIDERS_PREFIX,
};
use crate::models::Namespace;

impl Namespace {
    pub const ALL: [Namespace; 6] = [
        Namespace::Blocks,
        Namespace::Peers,
        Namespace::Pins,
        Namespace::Cache,
        Namespace::Provides,
        Namespace::RemoteProviders,
    ];

    pub fn prefix(&self) -> &'static [u8] {
        let prefix = match self {
            Namespace::Blocks => BLOCKS_PREFIX,
            Namespace::Peers => PEERS_PREFIX,
            Namespace::Pins => PINS_PREFIX,
            Namespace::Cache => CACHE_PREFIX,
            Namespace::Provides => PROVIDES_PREFIX,
            Namespace::RemoteProviders => REMOTE_PROVIDERS_PREFIX,
        };
        prefix.as_bytes()
    }

    /// Buduje pełny klucz bazy: prefiks + identyfikator wpisu.
    pub fn key(&self, id: &[u8]) -> Vec<u8> {
        let prefix = self.prefix();
        let mut key = Vec::with_capacity(prefix.len() + id.len());
        key.extend_from_slice(prefix);
        key.extend_from_slice(id);
        key
    }

    /// Sprawdza, czy pełny klucz bazy należy do tej przestrzeni nazw.
    pub fn owns(&self, key: &[u8]) -> bool {
        key.starts_with(self.prefix())
    }
}

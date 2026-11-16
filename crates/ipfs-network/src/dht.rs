use crate::consts::{DHT_MAX_PROVIDED_KEYS, DHT_QUERY_TIMEOUT_SECS, LAN_DHT_PROTOCOL};
use ipfs_cid::models::Cid;
use ipfs_store::utils::multihash_bytes;
use libp2p::{PeerId, StreamProtocol, kad};
use std::time::Duration;

pub type KademliaBehaviour = kad::Behaviour<kad::store::MemoryStore>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DhtMode {
    #[default]
    Client,
    Server,
}

impl DhtMode {
    pub fn libp2p_mode(self) -> kad::Mode {
        match self {
            Self::Client => kad::Mode::Client,
            Self::Server => kad::Mode::Server,
        }
    }
}

pub fn build_kademlia(local_peer_id: PeerId, mode: DhtMode) -> KademliaBehaviour {
    let protocol = StreamProtocol::new(LAN_DHT_PROTOCOL);
    let mut config = kad::Config::new(protocol);
    config.set_query_timeout(Duration::from_secs(DHT_QUERY_TIMEOUT_SECS));
    config.set_record_filtering(kad::StoreInserts::FilterBoth);
    let store_config = kad::store::MemoryStoreConfig {
        max_provided_keys: DHT_MAX_PROVIDED_KEYS,
        ..Default::default()
    };
    let store = kad::store::MemoryStore::with_config(local_peer_id, store_config);
    let mut kademlia = kad::Behaviour::with_config(local_peer_id, store, config);
    kademlia.set_mode(Some(mode.libp2p_mode()));
    kademlia
}

pub fn provider_key(cid: &Cid) -> Result<kad::RecordKey, &'static str> {
    if usize::from(cid.multihash.size) != cid.multihash.digest.len() {
        return Err("multihash length does not match digest");
    }
    Ok(kad::RecordKey::new(&multihash_bytes(&cid.multihash)))
}

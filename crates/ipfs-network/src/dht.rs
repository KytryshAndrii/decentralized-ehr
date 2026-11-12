use crate::consts::{DHT_QUERY_TIMEOUT_SECS, LAN_DHT_PROTOCOL};
use ipfs_cid::models::Cid;
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
    let store = kad::store::MemoryStore::new(local_peer_id);
    let mut kademlia = kad::Behaviour::with_config(local_peer_id, store, config);
    kademlia.set_mode(Some(mode.libp2p_mode()));
    kademlia
}

pub fn provider_key(cid: &Cid) -> Result<kad::RecordKey, &'static str> {
    if usize::from(cid.multihash.size) != cid.multihash.digest.len() {
        return Err("multihash length does not match digest");
    }
    let encoded_cid = cid.to_bytes();
    let (_, remaining) =
        unsigned_varint::decode::u64(&encoded_cid).map_err(|_| "invalid CID version encoding")?;
    let (_, multihash) =
        unsigned_varint::decode::u64(remaining).map_err(|_| "invalid CID codec encoding")?;
    Ok(kad::RecordKey::new(&multihash))
}

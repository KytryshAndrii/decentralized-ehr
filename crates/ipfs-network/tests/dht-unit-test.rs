use ipfs_cid::{
    consts::{CODEC_RAW, SHA2_256},
    models::Cid,
};
use ipfs_network::{
    DhtMode, NetworkConfig,
    consts::{DHT_QUERY_TIMEOUT_SECS, LAN_DHT_PROTOCOL},
    dht::{build_kademlia, provider_key},
};
use libp2p::{PeerId, identity::Keypair, kad};

#[test]
fn dht_uses_lan_protocol() {
    assert_eq!(LAN_DHT_PROTOCOL, "/ipfs/lan/kad/1.0.0");
    const { assert!(DHT_QUERY_TIMEOUT_SECS > 0) };
}

#[test]
fn client_is_default_and_server_can_be_selected() {
    assert_eq!(NetworkConfig::default().dht_mode, DhtMode::Client);
    assert_eq!(DhtMode::Client.libp2p_mode(), kad::Mode::Client);
    assert_eq!(DhtMode::Server.libp2p_mode(), kad::Mode::Server);
}

#[test]
fn kademlia_can_be_built_in_both_modes() {
    let peer = PeerId::from(Keypair::generate_ed25519().public());
    let _client = build_kademlia(peer, DhtMode::Client);
    let _server = build_kademlia(peer, DhtMode::Server);
}

#[test]
fn provider_key_is_multihash_without_cid_prefix() {
    let cid = Cid::generate_from_data(b"M5", CODEC_RAW, SHA2_256).unwrap();
    let key = provider_key(&cid).unwrap();
    let bytes: &[u8] = key.as_ref();
    assert_eq!(bytes.len(), 34);
    assert_eq!(bytes[0], 0x12);
    assert_eq!(bytes[1], 0x20);
    assert_eq!(&bytes[2..], cid.multihash.digest.as_slice());
}

#[test]
fn different_cid_codecs_share_provider_key_if_multihash_matches() {
    let raw = Cid::generate_from_data(b"same content", CODEC_RAW, SHA2_256).unwrap();
    let mut dag_pb = raw.clone();
    dag_pb.codec = 0x70;
    assert_eq!(provider_key(&raw).unwrap(), provider_key(&dag_pb).unwrap());
}

#[test]
fn invalid_multihash_length_is_rejected() {
    let mut cid = Cid::generate_from_data(b"bad size", CODEC_RAW, SHA2_256).unwrap();
    cid.multihash.size = 5;
    assert!(provider_key(&cid).is_err());
}

#[test]
fn provider_key_matches_blockstore_multihash_key() {
    let cid = Cid::generate_from_data(b"one rule", CODEC_RAW, SHA2_256).unwrap();
    let key = provider_key(&cid).unwrap();
    let bytes: &[u8] = key.as_ref();
    assert_eq!(bytes, ipfs_store::utils::multihash_bytes(&cid.multihash));
}

#[test]
fn memory_store_silently_drops_providers_over_the_per_key_limit() {
    use libp2p::kad::store::{MemoryStore, RecordStore};
    let mut store = MemoryStore::new(PeerId::random());
    let key = kad::RecordKey::new(&b"block".to_vec());
    let record = |provider| kad::ProviderRecord::new(key.clone(), provider, Vec::new());
    for _ in 0..kad::K_VALUE.get() {
        store.add_provider(record(PeerId::random())).unwrap();
    }
    let extra = PeerId::random();
    assert!(store.add_provider(record(extra)).is_ok());
    assert!(
        !store
            .providers(&key)
            .iter()
            .any(|kept| kept.provider == extra)
    );
}

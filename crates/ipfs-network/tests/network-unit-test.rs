use ipfs_network::{BootstrapPeer, NetworkConfig, NetworkNode, identity};
use libp2p::{PeerId, identity::Keypair};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let number = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ehr-network-test-{}-{number}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp directory");
        Self(path)
    }
    fn key(&self) -> PathBuf {
        self.0.join("nested/node.key.pb")
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn random_peer_id() -> PeerId {
    PeerId::from(Keypair::generate_ed25519().public())
}

#[test]
fn identity_is_created_and_reused() {
    let temp_dir = TempDir::new();
    let first = identity::load_or_create(&temp_dir.key()).unwrap();
    let data1 = fs::read(temp_dir.key()).unwrap();
    let second = identity::load_or_create(&temp_dir.key()).unwrap();
    assert_eq!(PeerId::from(first.public()), PeerId::from(second.public()));
    assert_eq!(data1, fs::read(temp_dir.key()).unwrap());
}
#[test]
fn different_key_files_produce_different_peer_ids() {
    let temp_dir = TempDir::new();
    let first = identity::load_or_create(&temp_dir.0.join("a.pb")).unwrap();
    let second = identity::load_or_create(&temp_dir.0.join("b.pb")).unwrap();
    assert_ne!(PeerId::from(first.public()), PeerId::from(second.public()));
}
#[test]
fn corrupt_identity_must_not_be_regenerated() {
    let temp_dir = TempDir::new();
    fs::create_dir_all(temp_dir.key().parent().unwrap()).unwrap();
    fs::write(
        temp_dir.key().parent().unwrap().join("corrupt.pb"),
        b"invalid protobuf",
    )
    .unwrap();
    let path = temp_dir.0.join("nested/corrupt.pb");
    assert!(identity::load_or_create(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"invalid protobuf");
}
#[test]
fn identity_path_as_directory_returns_error() {
    let temp_dir = TempDir::new();
    assert!(identity::load_or_create(&temp_dir.0).is_err());
}
#[test]
fn bootstrap_parse_strips_peer_id_and_preserves_transport() {
    let peer = random_peer_id();
    let input = format!("/ip4/127.0.0.1/tcp/4101/p2p/{peer}");
    let parsed = BootstrapPeer::parse(&input).unwrap();
    assert_eq!(parsed.peer_id, peer);
    assert_eq!(parsed.address.to_string(), "/ip4/127.0.0.1/tcp/4101");
}
#[test]
fn bootstrap_missing_peer_id_is_rejected() {
    assert!(BootstrapPeer::parse("/ip4/127.0.0.1/tcp/4101").is_err());
}
#[test]
fn bootstrap_missing_transport_is_rejected() {
    assert!(BootstrapPeer::parse(&format!("/p2p/{}", random_peer_id())).is_err());
}
#[test]
fn bootstrap_malformed_address_is_rejected() {
    assert!(BootstrapPeer::parse("not-a-multiaddr").is_err());
}
#[test]
fn bootstrap_list_parses_several_peers_and_drops_duplicates() {
    let (first, second) = (random_peer_id(), random_peer_id());
    let input = format!(
        "/ip4/127.0.0.1/tcp/4101/p2p/{first}, /ip4/127.0.0.1/tcp/4102/p2p/{second},/ip4/10.0.0.1/tcp/4101/p2p/{first}"
    );
    let parsed = BootstrapPeer::parse_list(&input).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].peer_id, first);
    assert_eq!(parsed[0].address.to_string(), "/ip4/127.0.0.1/tcp/4101");
    assert_eq!(parsed[1].peer_id, second);
}
#[test]
fn bootstrap_list_with_invalid_item_is_rejected() {
    let input = format!(
        "/ip4/127.0.0.1/tcp/4101/p2p/{},not-a-multiaddr",
        random_peer_id()
    );
    assert!(BootstrapPeer::parse_list(&input).is_err());
    assert!(BootstrapPeer::parse_list("").is_err());
}
#[test]
fn default_config_has_no_state_directory() {
    assert!(NetworkConfig::default().state_directory.is_none());
}
#[test]
fn default_config_listens_on_all_interfaces_and_backoff() {
    let config = NetworkConfig::default();
    assert_eq!(config.listen.to_string(), "/ip4/0.0.0.0/tcp/4101");
    assert!(config.bootstrap.is_empty());
    assert!(config.retry_initial < config.retry_max);
}
#[test]
fn node_rejects_invalid_retry_settings() {
    let temp_dir = TempDir::new();
    let config = NetworkConfig {
        identity_path: temp_dir.key(),
        retry_initial: Duration::ZERO,
        ..NetworkConfig::default()
    };
    assert!(NetworkNode::new_network_node(config.clone()).is_err());
    let config = NetworkConfig {
        identity_path: temp_dir.key(),
        retry_initial: Duration::from_secs(2),
        retry_max: Duration::from_secs(1),
        ..NetworkConfig::default()
    };
    assert!(NetworkNode::new_network_node(config.clone()).is_err());
    let config = NetworkConfig {
        identity_path: temp_dir.key(),
        retry_max: Duration::from_secs(30),
        maintenance_tick: Duration::ZERO,
        ..NetworkConfig::default()
    };
    assert!(NetworkNode::new_network_node(config).is_err());
}
#[test]
fn node_ignores_self_as_bootstrap() {
    let temp_dir = TempDir::new();
    let key = identity::load_or_create(&temp_dir.key()).unwrap();
    let peer = PeerId::from(key.public());
    let config = NetworkConfig {
        identity_path: temp_dir.key(),
        bootstrap: vec![
            BootstrapPeer::parse(&format!("/ip4/127.0.0.1/tcp/4111/p2p/{peer}")).unwrap(),
        ],
        ..NetworkConfig::default()
    };
    let node = NetworkNode::new_network_node(config).unwrap();
    assert_eq!(node.local_peer_id(), peer);
}
#[cfg(unix)]
#[test]
fn private_key_file_has_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = TempDir::new();
    identity::load_or_create(&temp_dir.key()).unwrap();
    let mode = fs::metadata(temp_dir.key()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

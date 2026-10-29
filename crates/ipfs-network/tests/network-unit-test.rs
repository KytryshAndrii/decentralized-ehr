use ipfs_network::{BootstrapPeer, NetworkConfig, NetworkNode, identity};
use libp2p::{PeerId, identity::Keypair};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static N: AtomicU64 = AtomicU64::new(0);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ehr-network-test-{}-{n}", std::process::id()));
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
fn id() -> PeerId {
    PeerId::from(Keypair::generate_ed25519().public())
}

#[test]
fn identity_is_created_and_reused() {
    let tmp = TempDir::new();
    let first = identity::load_or_create(&tmp.key()).unwrap();
    let data1 = fs::read(tmp.key()).unwrap();
    let second = identity::load_or_create(&tmp.key()).unwrap();
    assert_eq!(PeerId::from(first.public()), PeerId::from(second.public()));
    assert_eq!(data1, fs::read(tmp.key()).unwrap());
}
#[test]
fn different_key_files_produce_different_peer_ids() {
    let tmp = TempDir::new();
    let a = identity::load_or_create(&tmp.0.join("a.pb")).unwrap();
    let b = identity::load_or_create(&tmp.0.join("b.pb")).unwrap();
    assert_ne!(PeerId::from(a.public()), PeerId::from(b.public()));
}
#[test]
fn corrupt_identity_must_not_be_regenerated() {
    let tmp = TempDir::new();
    fs::create_dir_all(tmp.key().parent().unwrap()).unwrap();
    fs::write(
        tmp.key().parent().unwrap().join("corrupt.pb"),
        b"invalid protobuf",
    )
    .unwrap();
    let path = tmp.0.join("nested/corrupt.pb");
    assert!(identity::load_or_create(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"invalid protobuf");
}
#[test]
fn identity_path_as_directory_returns_error() {
    let tmp = TempDir::new();
    assert!(identity::load_or_create(&tmp.0).is_err());
}
#[test]
fn bootstrap_parse_strips_peer_id_and_preserves_transport() {
    let peer = id();
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
    assert!(BootstrapPeer::parse(&format!("/p2p/{}", id())).is_err());
}
#[test]
fn bootstrap_malformed_address_is_rejected() {
    assert!(BootstrapPeer::parse("not-a-multiaddr").is_err());
}
#[test]
fn default_config_uses_loopback_and_backoff() {
    let cfg = NetworkConfig::default();
    assert_eq!(cfg.listen.to_string(), "/ip4/127.0.0.1/tcp/0");
    assert!(cfg.bootstrap.is_empty());
    assert!(cfg.retry_initial < cfg.retry_max);
}
#[test]
fn node_rejects_invalid_retry_settings() {
    let tmp = TempDir::new();
    let cfg = NetworkConfig {
        identity_path: tmp.key(),
        retry_initial: Duration::ZERO,
        ..NetworkConfig::default()
    };
    assert!(NetworkNode::new_network_node(cfg.clone()).is_err());
    let cfg = NetworkConfig {
        identity_path: tmp.key(),
        retry_initial: Duration::from_secs(2),
        retry_max: Duration::from_secs(1),
        ..NetworkConfig::default()
    };
    assert!(NetworkNode::new_network_node(cfg.clone()).is_err());
    let cfg = NetworkConfig {
        identity_path: tmp.key(),
        retry_max: Duration::from_secs(30),
        maintenance_tick: Duration::ZERO,
        ..NetworkConfig::default()
    };
    assert!(NetworkNode::new_network_node(cfg).is_err());
}
#[test]
fn node_ignores_self_as_bootstrap() {
    let tmp = TempDir::new();
    let key = identity::load_or_create(&tmp.key()).unwrap();
    let peer = PeerId::from(key.public());
    let cfg = NetworkConfig {
        identity_path: tmp.key(),
        bootstrap: vec![
            BootstrapPeer::parse(&format!("/ip4/127.0.0.1/tcp/4111/p2p/{peer}")).unwrap(),
        ],
        ..NetworkConfig::default()
    };
    let node = NetworkNode::new_network_node(cfg).unwrap();
    assert_eq!(node.local_peer_id(), peer);
}
#[cfg(unix)]
#[test]
fn private_key_file_has_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new();
    identity::load_or_create(&tmp.key()).unwrap();
    let mode = fs::metadata(tmp.key()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

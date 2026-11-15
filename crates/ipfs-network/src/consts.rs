//This is a libp2p network identity, not a medical authorization role.
pub const DEFAULT_BOOTSTRAP_MULTIADDR: &str = concat!(
    "/ip4/192.168.1.125/tcp/4101/p2p/",
    "12D3KooWNX5AdkfjZ6nSWD8x7Cci1nuNtpXnj1AL42BXi7M9GENS"
);

// Local defaults.
// Every node listens on all IPv4 interfaces on the same port as DEFAULT_BOOTSTRAP,
// so a node started with --dht-server is reachable as a bootstrap peer.
pub const DEFAULT_LISTEN_MULTIADDR: &str = "/ip4/0.0.0.0/tcp/4101";
// Overrides only the TCP port; meant for running several nodes on one host (tests).
pub const LISTEN_PORT_ENV: &str = "EHR_LISTEN_PORT";
pub const DEFAULT_IDENTITY_PATH: &str = "./tmp/node-key.pb";
// Reconnect configuration.
pub const RETRY_INITIAL_SECS: u64 = 2;
pub const RETRY_MAX_SECS: u64 = 30;
pub const MAINTENANCE_TICK_SECS: u64 = 1;

// Protocol configuration.
pub const PING_INTERVAL_SECS: u64 = 15;
pub const IDLE_TIMEOUT_SECS: u64 = 90;
pub const LAN_DHT_PROTOCOL: &str = "/ipfs/lan/kad/1.0.0";
pub const DHT_QUERY_TIMEOUT_SECS: u64 = 30;

pub const REPROVIDE_INTERVAL_SECS: u64 = 12 * 60 * 60;
pub const BOOTSTRAP_LIST_SEPARATOR: char = ',';
pub const PROVIDER_RECORD_TTL_SECS: u64 = 48 * 60 * 60;
pub const PROVIDER_PERSIST_REFRESH_SECS: u64 = 60 * 60;
/// Znany peer niewidziany tyle czasu jest usuwany przy starcie.
// connected for over a week is dropped on the next restart; refresh on disconnect if that bites.
pub const KNOWN_PEER_TTL_SECS: u64 = 7 * 24 * 60 * 60;
pub const KNOWN_PEER_REFRESH_SECS: u64 = 24 * 60 * 60;

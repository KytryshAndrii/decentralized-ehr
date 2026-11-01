//This is a libp2p network identity, not a medical authorization role.
pub const DEFAULT_BOOTSTRAP_MULTIADDR: &str = concat!(
    "/ip4/192.168.1.125/tcp/4101/p2p/",
    "12D3KooWQ4LnED9zsSxm5Cv2TCEzt3f7kPxHLaJeKjSYYeJAV8FG"
);

// Local defaults.
pub const DEFAULT_LISTEN_MULTIADDR: &str = "/ip4/127.0.0.1/tcp/0";
pub const DEFAULT_IDENTITY_PATH: &str = "./node-key.pb";

// Reconnect configuration.
pub const RETRY_INITIAL_SECS: u64 = 2;
pub const RETRY_MAX_SECS: u64 = 30;
pub const MAINTENANCE_TICK_SECS: u64 = 1;

// Protocol configuration.
pub const PING_INTERVAL_SECS: u64 = 15;
pub const IDLE_TIMEOUT_SECS: u64 = 90;

use crate::consts::{
    BOOTSTRAP_LIST_SEPARATOR, DEFAULT_BOOTSTRAP_MULTIADDR, DEFAULT_IDENTITY_PATH,
    DEFAULT_LISTEN_MULTIADDR, IDLE_TIMEOUT_SECS, LISTEN_PORT_ENV, MAINTENANCE_TICK_SECS,
    PING_INTERVAL_SECS, REPROVIDE_INTERVAL_SECS, RETRY_INITIAL_SECS, RETRY_MAX_SECS,
};
use crate::dht::DhtMode;
use crate::error::NetworkError;
use crate::models::{BootstrapPeer, NetworkConfig};
use libp2p::{Multiaddr, multiaddr::Protocol};
use std::{path::PathBuf, time::Duration};
impl BootstrapPeer {
    pub fn parse(value: &str) -> Result<Self, NetworkError> {
        let mut address: Multiaddr = value.parse().map_err(|e| NetworkError::InvalidAddress {
            value: value.into(),
            reason: format!("{e}"),
        })?;
        let peer_id = match address.pop() {
            Some(Protocol::P2p(peer_id)) => peer_id,
            _ => {
                return Err(NetworkError::InvalidAddress {
                    value: value.into(),
                    reason: "expected trailing /p2p/<PeerId>".into(),
                });
            }
        };
        if address.is_empty() {
            return Err(NetworkError::InvalidAddress {
                value: value.into(),
                reason: "missing transport address".into(),
            });
        }
        Ok(Self { peer_id, address })
    }
    /// Parses a comma-separated list of bootstrap addresses; duplicates of one PeerId are dropped.
    pub fn parse_list(value: &str) -> Result<Vec<Self>, NetworkError> {
        let mut peers: Vec<Self> = Vec::new();
        for item in value.split(BOOTSTRAP_LIST_SEPARATOR) {
            let peer = Self::parse(item.trim())?;
            if !peers.iter().any(|known| known.peer_id == peer.peer_id) {
                peers.push(peer);
            }
        }
        Ok(peers)
    }
    /// Opt-in bootstrap contact; NetworkConfig::default() remains bootstrap-free.
    pub fn default_bootstrap() -> Result<Self, NetworkError> {
        Self::parse(DEFAULT_BOOTSTRAP_MULTIADDR)
    }
}
impl NetworkConfig {
    /// Hardcoded listen address, with the TCP port optionally taken from EHR_LISTEN_PORT.
    pub fn listen_from_env() -> Result<Multiaddr, NetworkError> {
        let mut address: Multiaddr = DEFAULT_LISTEN_MULTIADDR.parse().expect("static address");
        let Ok(value) = std::env::var(LISTEN_PORT_ENV) else {
            return Ok(address);
        };
        let port: u16 = value.parse().map_err(|e| NetworkError::InvalidAddress {
            value: format!("{LISTEN_PORT_ENV}={value}"),
            reason: format!("{e}"),
        })?;
        address.pop();
        address.push(Protocol::Tcp(port));
        Ok(address)
    }
}
impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN_MULTIADDR.parse().expect("static address"),
            identity_path: PathBuf::from(DEFAULT_IDENTITY_PATH),
            bootstrap: Vec::new(),
            retry_initial: Duration::from_secs(RETRY_INITIAL_SECS),
            retry_max: Duration::from_secs(RETRY_MAX_SECS),
            maintenance_tick: Duration::from_secs(MAINTENANCE_TICK_SECS),
            ping_interval: Duration::from_secs(PING_INTERVAL_SECS),
            idle_timeout: Duration::from_secs(IDLE_TIMEOUT_SECS),
            dht_mode: DhtMode::Client,
            state_directory: None,
            reprovide_interval: Duration::from_secs(REPROVIDE_INTERVAL_SECS),
        }
    }
}

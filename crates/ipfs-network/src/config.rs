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
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen: "/ip4/127.0.0.1/tcp/0".parse().expect("static address"),
            identity_path: PathBuf::from("./node-key.pb"),
            bootstrap: Vec::new(),
            retry_initial: Duration::from_secs(2),
            retry_max: Duration::from_secs(30),
            maintenance_tick: Duration::from_secs(1),
            ping_interval: Duration::from_secs(15),
            idle_timeout: Duration::from_secs(90),
        }
    }
}

use crate::behaviour::NetworkBehaviour;
use libp2p::{Multiaddr, PeerId, Swarm};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub(crate) struct RetryState {
    pub(crate) peer: BootstrapPeer,
    pub(crate) connected: bool,
    pub(crate) dialing: bool,
    pub(crate) next_attempt: Instant,
    pub(crate) delay: Duration,
}

pub struct NetworkNode {
    pub(crate) swarm: Swarm<NetworkBehaviour>,
    pub(crate) config: NetworkConfig,
    pub(crate) retries: HashMap<PeerId, RetryState>,
}

/// Peers from this list are direct addresses, not DHT-discovered peers.
#[derive(Clone, Debug)]
pub struct BootstrapPeer {
    pub peer_id: PeerId,
    pub address: Multiaddr,
}

#[derive(Clone, Debug)]
pub struct NetworkConfig {
    pub listen: Multiaddr,
    pub identity_path: PathBuf,
    pub bootstrap: Vec<BootstrapPeer>,
    pub retry_initial: Duration,
    pub retry_max: Duration,
    pub maintenance_tick: Duration,
    pub ping_interval: Duration,
    pub idle_timeout: Duration,
}

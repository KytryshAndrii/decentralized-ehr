use crate::behaviour::NetworkBehaviour;
use crate::dht::DhtMode;
use ipfs_cid::models::Cid;
use ipfs_store::KnownPeer;
use libp2p::{Multiaddr, PeerId, Swarm, identity::Keypair, kad};
use std::collections::{HashMap, HashSet};
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
    pub(crate) dht_bootstrap_started: bool,
    pub(crate) pending_provider_queries: HashMap<kad::QueryId, HashSet<PeerId>>,
    pub(crate) known_peers: HashMap<PeerId, KnownPeer>,
    pub(crate) provided: HashMap<Vec<u8>, Cid>,
    pub(crate) remote_providers: HashMap<(Vec<u8>, PeerId), u64>,
    pub(crate) next_reprovide: Instant,
    pub(crate) reprovided_after_bootstrap: bool,
    pub(crate) bitswap_client: Option<ipfs_bitswap::BitswapClient>,
    /// Zapytania DHT o dostawców, po których blok ma zostać pobrany przez Bitswap.
    pub(crate) pending_fetches: HashMap<kad::QueryId, (Cid, Option<FetchReply>)>,
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
    pub dht_mode: DhtMode,
    pub state_directory: Option<PathBuf>,
    pub reprovide_interval: Duration,
    /// Gotowy klucz węzła (np. odszyfrowany hasłem przez warstwę EHR).
    /// `None`: klucz z `identity_path`, jak dotąd.
    pub keypair: Option<Keypair>,
}

/// Wynik pobrania całego pliku zleconego przez `NetworkCommand::FetchWithReply`.
pub type FetchReply = tokio::sync::oneshot::Sender<Result<(), String>>;

/// Uchwyt do otwierania i przyjmowania strumieni libp2p na Swarmie węzła
/// (ten sam `libp2p-stream`, którego używa Bitswap; inne protokoły dostają własne nazwy).
pub type StreamControl = ipfs_bitswap::BitswapControl;

#[derive(Debug)]
pub enum NetworkCommand {
    StartProviding(Cid),
    GetProviders(Cid),
    StopProviding(Cid),
    /// Znajdź dostawców w DHT, pobierz blok przez Bitswap i zapisz go w Blockstore.
    Fetch(Cid),
    /// Jak `Fetch`, ale wynik (cały plik lokalnie albo błąd) wraca kanałem.
    FetchWithReply(Cid, FetchReply),
    /// Dodaje znany adres peera do zbioru połączeń utrzymywanych przez węzeł (jak bootstrap).
    AddPeer(BootstrapPeer),
    Bootstrap,
    ListPeers,
    Quit,
}

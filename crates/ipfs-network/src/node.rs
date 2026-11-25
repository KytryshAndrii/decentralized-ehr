use crate::models::{
    BootstrapPeer, NetworkCommand, NetworkConfig, NetworkNode, RetryState, StreamControl,
};
use crate::{
    behaviour::NetworkBehaviour,
    dht::{build_kademlia, provider_key},
    error::NetworkError,
    identity,
    utils::error_chain,
};
use futures::StreamExt;
use ipfs_cid::models::Cid;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{PeerId, SwarmBuilder, identify, kad, ping};
use std::{collections::HashMap, time::Instant};
use tokio::sync::mpsc;

impl NetworkNode {
    pub fn new_network_node(config: NetworkConfig) -> Result<Self, NetworkError> {
        if config.retry_initial.is_zero()
            || config.maintenance_tick.is_zero()
            || config.retry_max < config.retry_initial
        {
            return Err(NetworkError::Libp2p("invalid retry durations".into()));
        }
        let keypair = match &config.keypair {
            Some(keypair) => keypair.clone(),
            None => identity::load_or_create(&config.identity_path)?,
        };
        let local_id = PeerId::from(keypair.public());
        let ping_config = ping::Config::new().with_interval(config.ping_interval);
        let mut swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )
            .map_err(|error| NetworkError::Libp2p(error.to_string()))?
            .with_behaviour(|key| NetworkBehaviour {
                identify: identify::Behaviour::new(identify::Config::new(
                    "/ipfs/0.1.0".into(),
                    key.public(),
                )),
                ping: ping::Behaviour::new(ping_config),
                kad: build_kademlia(local_id, config.dht_mode),
                bitswap: ipfs_bitswap::new_behaviour(),
            })
            .map_err(|error| NetworkError::Libp2p(error.to_string()))?
            .with_swarm_config(|swarm_config| {
                swarm_config.with_idle_connection_timeout(config.idle_timeout)
            })
            .build();
        let mut retries = HashMap::new();
        for peer in &config.bootstrap {
            if peer.peer_id == local_id {
                continue;
            }
            swarm
                .behaviour_mut()
                .kad
                .add_address(&peer.peer_id, peer.address.clone());
            retries.insert(
                peer.peer_id,
                RetryState {
                    peer: peer.clone(),
                    connected: false,
                    dialing: false,
                    next_attempt: Instant::now(),
                    delay: config.retry_initial,
                },
            );
        }
        Ok(Self {
            swarm,
            config,
            retries,
            dht_bootstrap_started: false,
            pending_provider_queries: HashMap::new(),
            known_peers: HashMap::new(),
            provided: HashMap::new(),
            remote_providers: HashMap::new(),
            next_reprovide: Instant::now(),
            reprovided_after_bootstrap: false,
            bitswap_client: None,
            pending_fetches: HashMap::new(),
        })
    }
    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    /// Stream handle for application protocols; take it before `run_with_commands`.
    pub fn stream_control(&self) -> StreamControl {
        self.swarm.behaviour().bitswap.new_control()
    }

    pub fn start_providing(&mut self, cid: &Cid) -> Result<kad::QueryId, NetworkError> {
        let key = provider_key(cid).map_err(|error| NetworkError::Libp2p(error.into()))?;
        self.swarm
            .behaviour_mut()
            .kad
            .start_providing(key)
            .map_err(|error| NetworkError::Libp2p(error.to_string()))
    }

    pub fn get_providers(&mut self, cid: &Cid) -> Result<kad::QueryId, NetworkError> {
        let key = provider_key(cid).map_err(|error| NetworkError::Libp2p(error.into()))?;
        let id = self.swarm.behaviour_mut().kad.get_providers(key);
        self.pending_provider_queries.insert(id, Default::default());
        Ok(id)
    }

    pub fn stop_providing(&mut self, cid: &Cid) -> Result<(), NetworkError> {
        let key = provider_key(cid).map_err(|error| NetworkError::Libp2p(error.into()))?;
        self.swarm.behaviour_mut().kad.stop_providing(&key);
        Ok(())
    }

    pub fn listen(&mut self) -> Result<(), NetworkError> {
        let address = self.config.listen.clone();
        self.swarm.listen_on(address.clone()).map_err(|error| {
            NetworkError::Libp2p(format!(
                "cannot listen on {address}: {}",
                error_chain(&error)
            ))
        })?;
        Ok(())
    }
    /// Add a known peer to the reconnect set; this is not automatic DHT discovery.
    pub fn add_bootstrap(&mut self, peer: BootstrapPeer) {
        if peer.peer_id == self.local_peer_id() {
            return;
        }
        self.swarm
            .behaviour_mut()
            .kad
            .add_address(&peer.peer_id, peer.address.clone());
        self.retries.insert(
            peer.peer_id,
            RetryState {
                peer,
                connected: false,
                dialing: false,
                next_attempt: Instant::now(),
                delay: self.config.retry_initial,
            },
        );
    }
    fn attempt_connections(&mut self) {
        let now = Instant::now();

        for state in self.retries.values_mut() {
            if state.connected || state.dialing || now < state.next_attempt {
                continue;
            }

            let peer_id = state.peer.peer_id;
            let address = state.peer.address.clone();

            println!("DIAL EXPECTED: {peer_id}");
            println!("DIAL ADDRESS:  {address}");
            let options = DialOpts::peer_id(peer_id).addresses(vec![address]).build();

            if self.swarm.is_connected(&peer_id) {
                state.connected = true;
                state.dialing = false;
                continue;
            }
            state.dialing = true;

            if let Err(error) = self.swarm.dial(options) {
                eprintln!("DIAL_FAILED {peer_id}: {error}");
                state.dialing = false;
                state.next_attempt = now + state.delay;
                state.delay = state.delay.saturating_mul(2).min(self.config.retry_max);
            }
        }
    }
    pub(crate) fn schedule_retry(&mut self, peer_id: PeerId) {
        if let Some(state) = self.retries.get_mut(&peer_id) {
            state.connected = false;
            state.dialing = false;
            state.next_attempt = Instant::now() + state.delay;
            state.delay = state.delay.saturating_mul(2).min(self.config.retry_max);
            println!(
                "retry scheduled for {peer_id} in {:?}",
                state.next_attempt.saturating_duration_since(Instant::now())
            );
        }
    }
    pub async fn run(self) -> Result<(), NetworkError> {
        self.run_inner(None).await
    }
    pub async fn run_with_commands(
        self,
        commands: mpsc::UnboundedReceiver<NetworkCommand>,
    ) -> Result<(), NetworkError> {
        self.run_inner(Some(commands)).await
    }
    async fn run_inner(
        mut self,
        mut commands: Option<mpsc::UnboundedReceiver<NetworkCommand>>,
    ) -> Result<(), NetworkError> {
        self.listen()?;
        println!("LOCAL_PEER_ID {}", self.local_peer_id());
        self.restore_state();
        self.start_bitswap();
        let mut ticker = tokio::time::interval(self.config.maintenance_tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.attempt_connections();
                    self.reprovide_if_due();
                }
                event = self.swarm.select_next_some() => self.on_swarm_event(event),
                command = async {
                    match commands.as_mut() {
                        Some(receiver) => receiver.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match command {
                        Some(NetworkCommand::Quit) => return Ok(()),
                        Some(command) => self.on_command(command),
                        None => commands = None,
                    }
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    println!("Shutdown requested");
                    return Ok(());
                }
            }
        }
    }
}

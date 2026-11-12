use crate::models::{BootstrapPeer, NetworkCommand, NetworkConfig, NetworkNode, RetryState};
use crate::{
    behaviour::{NetworkBehaviour, NetworkEvent},
    consts::LAN_DHT_PROTOCOL,
    dht::{build_kademlia, provider_key},
    error::NetworkError,
    identity,
};
use futures::StreamExt;
use ipfs_cid::models::Cid;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{PeerId, SwarmBuilder, identify, kad, multiaddr::Protocol, ping, swarm::SwarmEvent};
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
        let keypair = identity::load_or_create(&config.identity_path)?;
        let local_id = PeerId::from(keypair.public());
        let ping_config = ping::Config::new().with_interval(config.ping_interval);
        let mut swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )
            .map_err(|e| NetworkError::Libp2p(e.to_string()))?
            .with_behaviour(|key| NetworkBehaviour {
                identify: identify::Behaviour::new(identify::Config::new(
                    "/ipfs/0.1.0".into(),
                    key.public(),
                )),
                ping: ping::Behaviour::new(ping_config),
                kad: build_kademlia(local_id, config.dht_mode),
            })
            .map_err(|e| NetworkError::Libp2p(e.to_string()))?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
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
        })
    }
    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
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
        self.swarm
            .listen_on(self.config.listen.clone())
            .map_err(|e| NetworkError::Libp2p(e.to_string()))?;
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
    fn schedule_retry(&mut self, peer_id: PeerId) {
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
    fn on_swarm_event(&mut self, event: SwarmEvent<NetworkEvent>) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("LISTEN {address}/p2p/{}", self.local_peer_id());
                let externally_usable = address.iter().all(|component| match component {
                    Protocol::Ip4(ip) => !ip.is_unspecified(),
                    Protocol::Ip6(ip) => !ip.is_unspecified(),
                    _ => true,
                });
                if externally_usable {
                    self.swarm.add_external_address(address);
                }
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("CONNECTED {peer_id}");
                if let Some(state) = self.retries.get_mut(&peer_id) {
                    state.connected = true;
                    state.dialing = false;
                    state.delay = self.config.retry_initial;
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                num_established,
                ..
            } => {
                println!("DISCONNECTED {peer_id}");
                if num_established == 0 {
                    self.schedule_retry(peer_id);
                }
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error,
                ..
            } => {
                if let libp2p::swarm::DialError::WrongPeerId { obtained, address } = &error {
                    eprintln!("WRONG PEER ID:");
                    eprintln!("  Expected: {peer_id}");
                    eprintln!("  Obtained: {obtained}");
                    eprintln!("  Match:    {}", peer_id == *obtained);
                    eprintln!("  Address:  {address}");
                } else {
                    eprintln!("DIAL_FAILED {peer_id}: {error}");
                }
                self.schedule_retry(peer_id);
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: None,
                error,
                ..
            } => {
                eprintln!("DIAL_FAILED unknown peer: {error}");
                // All configured addresses end in /p2p/<PeerId>; normally peer_id is Some.
            }
            SwarmEvent::Behaviour(NetworkEvent::Identify(event)) => {
                if let identify::Event::Received { peer_id, info, .. } = *event {
                    println!(
                        "IDENTIFY {peer_id}: agent={} protocols={:?} listen={:?}",
                        info.agent_version, info.protocols, info.listen_addrs
                    );
                    let supports_dht = info
                        .protocols
                        .iter()
                        .any(|protocol| protocol.as_ref() == LAN_DHT_PROTOCOL);
                    if supports_dht {
                        for address in &info.listen_addrs {
                            self.swarm
                                .behaviour_mut()
                                .kad
                                .add_address(&peer_id, address.clone());
                        }
                        self.start_dht_bootstrap();
                    }
                }
            }
            SwarmEvent::Behaviour(NetworkEvent::Ping(ping::Event { peer, result, .. })) => {
                match result {
                    Ok(rtt) => println!("PING {peer}: {rtt:?}"),
                    Err(error) => eprintln!("PING_FAILED {peer}: {error}"),
                }
            }
            SwarmEvent::Behaviour(NetworkEvent::Kad(event)) => self.on_dht_event(*event),
            SwarmEvent::ListenerError { error, .. } => eprintln!("LISTENER_ERROR: {error}"),
            SwarmEvent::ExpiredListenAddr { address, .. } => println!("EXPIRED_LISTEN {address}"),
            _ => {}
        }
    }
    fn start_dht_bootstrap(&mut self) {
        if self.dht_bootstrap_started {
            return;
        }
        match self.swarm.behaviour_mut().kad.bootstrap() {
            Ok(_) => {
                self.dht_bootstrap_started = true;
                println!("DHT_BOOTSTRAP_STARTED");
            }
            Err(error) => eprintln!("DHT_BOOTSTRAP_FAILED {error:?}"),
        }
    }
    fn on_dht_event(&mut self, event: kad::Event) {
        match event {
            kad::Event::RoutingUpdated {
                peer, is_new_peer, ..
            } => {
                println!("DHT_ROUTING_UPDATED {peer} new={is_new_peer}");
            }
            kad::Event::OutboundQueryProgressed {
                id, result, step, ..
            } => {
                match result {
                    kad::QueryResult::Bootstrap(Ok(progress)) => {
                        println!(
                            "DHT_BOOTSTRAP_PROGRESS {} remaining={}",
                            progress.peer, progress.num_remaining
                        );
                    }
                    kad::QueryResult::Bootstrap(Err(error)) => {
                        eprintln!("DHT_BOOTSTRAP_FAILED {error:?}");
                        self.dht_bootstrap_started = false;
                    }
                    kad::QueryResult::StartProviding(Ok(result)) => {
                        println!("DHT_PROVIDING_STARTED {:?}", result.key);
                    }
                    kad::QueryResult::StartProviding(Err(error)) => {
                        eprintln!("DHT_PROVIDING_FAILED {error:?}");
                    }
                    kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                        providers,
                        ..
                    })) => {
                        if let Some(found) = self.pending_provider_queries.get_mut(&id) {
                            for peer in providers {
                                if peer != *self.swarm.local_peer_id() && found.insert(peer) {
                                    println!("DHT_PROVIDER {peer}");
                                }
                            }
                        }
                    }
                    kad::QueryResult::GetProviders(Err(error)) => {
                        eprintln!("DHT_FIND_FAILED {error:?}");
                    }
                    _ => {}
                }
                if step.last
                    && let Some(providers) = self.pending_provider_queries.remove(&id)
                {
                    println!("DHT_FIND_FINISHED count={}", providers.len());
                }
            }
            _ => {}
        }
    }
    fn on_command(&mut self, command: NetworkCommand) {
        match command {
            NetworkCommand::StartProviding(cid) => match self.start_providing(&cid) {
                Ok(_) => println!("DHT_PROVIDE_REQUESTED {}", cid.to_string_base32()),
                Err(error) => eprintln!("DHT_PROVIDE_FAILED {error}"),
            },
            NetworkCommand::GetProviders(cid) => match self.get_providers(&cid) {
                Ok(_) => println!("DHT_FIND_REQUESTED {}", cid.to_string_base32()),
                Err(error) => eprintln!("DHT_FIND_FAILED {error}"),
            },
            NetworkCommand::StopProviding(cid) => match self.stop_providing(&cid) {
                Ok(()) => println!("DHT_STOPPED {}", cid.to_string_base32()),
                Err(error) => eprintln!("DHT_STOP_FAILED {error}"),
            },
            NetworkCommand::Bootstrap => {
                self.dht_bootstrap_started = false;
                self.start_dht_bootstrap();
            }
            NetworkCommand::ListPeers => {
                for bucket in self.swarm.behaviour_mut().kad.kbuckets() {
                    for entry in bucket.iter() {
                        println!("DHT_PEER {}", entry.node.key.preimage());
                    }
                }
                println!("DHT_PEERS_DONE");
            }
            NetworkCommand::Quit => {}
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
        let mut ticker = tokio::time::interval(self.config.maintenance_tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => self.attempt_connections(),
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

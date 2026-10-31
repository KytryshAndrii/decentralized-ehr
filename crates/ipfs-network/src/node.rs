use crate::models::{BootstrapPeer, NetworkConfig, NetworkNode, RetryState};
use crate::{
    behaviour::{NetworkBehaviour, NetworkEvent},
    error::NetworkError,
    identity,
};
use futures::StreamExt;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{PeerId, SwarmBuilder, identify, ping, swarm::SwarmEvent};
use std::{collections::HashMap, time::Instant};

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
        let swarm = SwarmBuilder::with_existing_identity(keypair)
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
            })
            .map_err(|e| NetworkError::Libp2p(e.to_string()))?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
            .build();
        let mut retries = HashMap::new();
        for peer in &config.bootstrap {
            if peer.peer_id == local_id {
                continue;
            }
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
        })
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
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
                }
            }
            SwarmEvent::Behaviour(NetworkEvent::Ping(ping::Event { peer, result, .. })) => {
                match result {
                    Ok(rtt) => println!("PING {peer}: {rtt:?}"),
                    Err(error) => eprintln!("PING_FAILED {peer}: {error}"),
                }
            }
            SwarmEvent::ListenerError { error, .. } => eprintln!("LISTENER_ERROR: {error}"),
            SwarmEvent::ExpiredListenAddr { address, .. } => println!("EXPIRED_LISTEN {address}"),
            _ => {}
        }
    }

    pub async fn run(mut self) -> Result<(), NetworkError> {
        self.listen()?;
        println!("LOCAL_PEER_ID {}", self.local_peer_id());
        let mut ticker = tokio::time::interval(self.config.maintenance_tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => self.attempt_connections(),
                event = self.swarm.select_next_some() => self.on_swarm_event(event),
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    println!("Shutdown requested");
                    return Ok(());
                }
            }
        }
    }
}

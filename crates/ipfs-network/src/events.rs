//! Swarm and Kademlia events of the node event loop.
use crate::behaviour::NetworkEvent;
use crate::consts::LAN_DHT_PROTOCOL;
use crate::models::NetworkNode;
use crate::utils::is_dialable;
use libp2p::{PeerId, identify, kad, ping, swarm::SwarmEvent};

impl NetworkNode {
    pub(crate) fn on_swarm_event(&mut self, event: SwarmEvent<NetworkEvent>) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("LISTEN {address}/p2p/{}", self.local_peer_id());
                if is_dialable(&address) {
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
                    } else if self
                        .swarm
                        .behaviour_mut()
                        .kad
                        .remove_peer(&peer_id)
                        .is_some()
                    {
                        // The peer switched to DHT client mode (e.g. a patient on low battery).
                        println!("DHT_PEER_REMOVED {peer_id}");
                    }
                    self.remember_peer(peer_id, &info.listen_addrs, supports_dht);
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
    pub(crate) fn start_dht_bootstrap(&mut self) {
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
            kad::Event::InboundRequest {
                request:
                    kad::InboundRequest::AddProvider {
                        record: Some(record),
                    },
            } => self.accept_remote_provider(record),
            kad::Event::OutboundQueryProgressed {
                id, result, step, ..
            } => {
                match result {
                    kad::QueryResult::Bootstrap(Ok(progress)) => {
                        println!(
                            "DHT_BOOTSTRAP_PROGRESS {} remaining={}",
                            progress.peer, progress.num_remaining
                        );
                        // The routing table is filled now, so the records reach the network.
                        if progress.num_remaining == 0 && !self.reprovided_after_bootstrap {
                            self.reprovided_after_bootstrap = true;
                            self.reprovide();
                        }
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
                    if let Some((cid, reply)) = self.pending_fetches.remove(&id) {
                        // DHT providers first, then connected peers, because the provider record may
                        // not have reached the DHT yet. Kubo does the opposite: it sends want-have to
                        // connected peers at once and asks the DHT in parallel after a short delay.
                        let mut candidates: Vec<PeerId> = providers.into_iter().collect();
                        for peer in self.swarm.connected_peers() {
                            if !candidates.contains(peer) {
                                candidates.push(*peer);
                            }
                        }
                        self.fetch_block(cid, candidates, reply);
                    }
                }
            }
            _ => {}
        }
    }
}

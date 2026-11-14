use crate::consts::{PROVIDER_PERSIST_REFRESH_SECS, PROVIDER_RECORD_TTL_SECS};
use crate::models::NetworkNode;
use ipfs_cid::models::Cid;
use ipfs_store::utils::utc_now_secs;
use ipfs_store::{KnownPeer, PersistentBlockstore, RemoteProvider};
use libp2p::kad::{self, store::RecordStore};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{Multiaddr, PeerId, multiaddr::Protocol};
use std::time::{Duration, Instant};

/// Trwały stan węzła: znane peery i ogłaszane CID.
impl NetworkNode {
    fn open_state_store(&self) -> Option<PersistentBlockstore> {
        let directory = self.config.state_directory.as_ref()?;
        match PersistentBlockstore::open_in(directory) {
            Ok(store) => Some(store),
            Err(error) => {
                eprintln!("STATE_STORE_FAILED {error}");
                None
            }
        }
    }

    pub(crate) fn restore_state(&mut self) {
        let Some(store) = self.open_state_store() else {
            return;
        };
        let peers = store.load_peers();
        let provides = store.list_provides();
        let now_utc = utc_now_secs();
        let expired = store.remove_expired_remote_providers(now_utc);
        let remote_providers = store.load_remote_providers(now_utc);
        drop(store);

        match expired {
            Ok(0) => {}
            Ok(removed) => println!("DHT_PROVIDER_RECORDS_EXPIRED count={removed}"),
            Err(error) => eprintln!("STATE_STORE_FAILED {error}"),
        }
        match remote_providers {
            Ok(providers) => {
                let mut restored = 0;
                for provider in providers {
                    if self.restore_remote_provider(provider, now_utc) {
                        restored += 1;
                    }
                }
                println!("DHT_PROVIDER_RECORDS_RESTORED count={restored}");
            }
            Err(error) => eprintln!("STATE_STORE_FAILED {error}"),
        }

        match peers {
            Ok(peers) => {
                let mut restored = 0;
                for peer in peers {
                    if self.restore_peer(peer) {
                        restored += 1;
                    }
                }
                println!("KNOWN_PEERS_RESTORED count={restored}");
            }
            Err(error) => eprintln!("STATE_STORE_FAILED {error}"),
        }
        match provides {
            Ok(cids) => {
                for cid in cids {
                    self.provided.insert(cid.to_bytes(), cid);
                }
                self.reprovide();
            }
            Err(error) => eprintln!("STATE_STORE_FAILED {error}"),
        }
    }

    fn restore_peer(&mut self, peer: KnownPeer) -> bool {
        let Ok(peer_id) = peer.peer_id.parse::<PeerId>() else {
            eprintln!("KNOWN_PEER_INVALID {}", peer.peer_id);
            return false;
        };
        let addresses: Vec<Multiaddr> = peer
            .addresses
            .iter()
            .filter_map(|address| address.parse().ok())
            .collect();
        if peer_id == self.local_peer_id() || addresses.is_empty() {
            return false;
        }
        println!(
            "KNOWN_PEER_RESTORED {peer_id} dht_server={}",
            peer.dht_server
        );
        if peer.dht_server {
            for address in &addresses {
                self.swarm
                    .behaviour_mut()
                    .kad
                    .add_address(&peer_id, address.clone());
            }
        }
        self.known_peers.insert(peer_id, peer);
        if !self.retries.contains_key(&peer_id) {
            let options = DialOpts::peer_id(peer_id).addresses(addresses).build();
            if let Err(error) = self.swarm.dial(options) {
                eprintln!("DIAL_FAILED {peer_id}: {error}");
            }
        }
        true
    }

    /// Wkłada zapisany rekord „peer ma blok” z powrotem do MemoryStore Kademlii.
    fn restore_remote_provider(&mut self, provider: RemoteProvider, now_utc: u64) -> bool {
        let Ok(peer_id) = provider.peer_id.parse::<PeerId>() else {
            return false;
        };
        let remaining = Duration::from_secs(provider.expires_at_utc.saturating_sub(now_utc));
        let record = kad::ProviderRecord {
            key: kad::RecordKey::new(&provider.multihash),
            provider: peer_id,
            expires: Some(Instant::now() + remaining),
            addresses: provider
                .addresses
                .iter()
                .filter_map(|address| address.parse().ok())
                .collect(),
        };
        let store = self.swarm.behaviour_mut().kad.store_mut();
        if let Err(error) = store.add_provider(record) {
            eprintln!("DHT_PROVIDER_RECORD_REJECTED {peer_id}: {error}");
            return false;
        }
        self.remote_providers
            .insert((provider.multihash, peer_id), provider.expires_at_utc);
        true
    }

    /// Obsługa ADD_PROVIDER od innego peera: rekord trafia do MemoryStore i na dysk.
    pub(crate) fn accept_remote_provider(&mut self, record: kad::ProviderRecord) {
        let peer_id = record.provider;
        let multihash = record.key.to_vec();
        let now_utc = utc_now_secs();
        let ttl_secs = record
            .expires
            .map(|expires| expires.saturating_duration_since(Instant::now()).as_secs())
            .unwrap_or(PROVIDER_RECORD_TTL_SECS);
        let expires_at_utc = now_utc + ttl_secs;
        let addresses: Vec<String> = record.addresses.iter().map(ToString::to_string).collect();

        let store = self.swarm.behaviour_mut().kad.store_mut();
        if let Err(error) = store.add_provider(record) {
            eprintln!("DHT_PROVIDER_RECORD_REJECTED {peer_id}: {error}");
            return;
        }
        let cache_key = (multihash, peer_id);
        let fresh_enough = self
            .remote_providers
            .get(&cache_key)
            .is_some_and(|saved| saved + PROVIDER_PERSIST_REFRESH_SECS > expires_at_utc);
        if fresh_enough {
            return;
        }
        let Some(store) = self.open_state_store() else {
            return;
        };
        let provider = RemoteProvider {
            multihash: cache_key.0.clone(),
            peer_id: peer_id.to_string(),
            addresses,
            expires_at_utc,
        };
        match store.save_remote_provider(&provider) {
            Ok(()) => {
                println!("DHT_PROVIDER_RECORD_SAVED {peer_id} expires_utc={expires_at_utc}");
                self.remote_providers.insert(cache_key, expires_at_utc);
            }
            Err(error) => eprintln!("STATE_STORE_FAILED {error}"),
        }
    }

    /// Zapisuje peera poznanego przez Identify; zapis tylko przy zmianie danych.
    pub(crate) fn remember_peer(
        &mut self,
        peer_id: PeerId,
        listen_addresses: &[Multiaddr],
        dht_server: bool,
    ) {
        let addresses: Vec<String> = listen_addresses
            .iter()
            .filter(|address| is_dialable(address))
            .map(ToString::to_string)
            .collect();
        if addresses.is_empty() {
            return;
        }
        let peer = KnownPeer {
            peer_id: peer_id.to_string(),
            addresses,
            dht_server,
        };
        if self.known_peers.get(&peer_id) == Some(&peer) {
            return;
        }
        let Some(store) = self.open_state_store() else {
            return;
        };
        match store.save_peer(&peer) {
            Ok(()) => {
                println!("KNOWN_PEER_SAVED {peer_id} dht_server={dht_server}");
                self.known_peers.insert(peer_id, peer);
            }
            Err(error) => eprintln!("STATE_STORE_FAILED {error}"),
        }
    }

    pub(crate) fn remember_provide(&mut self, cid: &Cid) {
        self.provided.insert(cid.to_bytes(), cid.clone());
        if let Some(store) = self.open_state_store()
            && let Err(error) = store.add_provide(cid)
        {
            eprintln!("STATE_STORE_FAILED {error}");
        }
    }

    pub(crate) fn forget_provide(&mut self, cid: &Cid) {
        self.provided.remove(&cid.to_bytes());
        if let Some(store) = self.open_state_store()
            && let Err(error) = store.remove_provide(cid)
        {
            eprintln!("STATE_STORE_FAILED {error}");
        }
    }

    /// Ponownie ogłasza w DHT wszystkie CID tego węzła.
    pub(crate) fn reprovide(&mut self) {
        self.next_reprovide = Instant::now() + self.config.reprovide_interval;
        if self.provided.is_empty() {
            return;
        }
        let cids: Vec<Cid> = self.provided.values().cloned().collect();
        let mut requested = 0;
        for cid in &cids {
            match self.start_providing(cid) {
                Ok(_) => requested += 1,
                Err(error) => eprintln!("DHT_PROVIDE_FAILED {error}"),
            }
        }
        println!("DHT_REPROVIDE count={requested}");
    }

    pub(crate) fn reprovide_if_due(&mut self) {
        if Instant::now() >= self.next_reprovide {
            self.reprovide();
        }
    }
}

fn is_dialable(address: &Multiaddr) -> bool {
    address.iter().all(|component| match component {
        Protocol::Ip4(ip) => !ip.is_unspecified(),
        Protocol::Ip6(ip) => !ip.is_unspecified(),
        _ => true,
    })
}

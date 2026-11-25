//! Commands sent to the node from the CLI, the desktop app and `ehr-core`.
use crate::models::{FetchReply, NetworkCommand, NetworkNode};
use ipfs_cid::models::Cid;
use libp2p::PeerId;

impl NetworkNode {
    /// Peers in the Kademlia routing table; only DHT servers get there.
    fn dht_peers(&mut self) -> Vec<PeerId> {
        let mut peers = Vec::new();
        for bucket in self.swarm.behaviour_mut().kad.kbuckets() {
            peers.extend(bucket.iter().map(|entry| *entry.node.key.preimage()));
        }
        peers
    }

    pub(crate) fn on_command(&mut self, command: NetworkCommand) {
        match command {
            NetworkCommand::StartProviding(cid) => {
                if !self.remember_provide(&cid) {
                    eprintln!("DHT_PROVIDE_FAILED state not saved");
                    return;
                }
                match self.start_providing(&cid) {
                    Ok(_) => println!("DHT_PROVIDE_REQUESTED {}", cid.to_string_base32()),
                    Err(error) => {
                        eprintln!("DHT_PROVIDE_FAILED {error}");
                        self.forget_provide(&cid);
                    }
                }
            }
            NetworkCommand::GetProviders(cid) => match self.get_providers(&cid) {
                Ok(_) => println!("DHT_FIND_REQUESTED {}", cid.to_string_base32()),
                Err(error) => eprintln!("DHT_FIND_FAILED {error}"),
            },
            NetworkCommand::Fetch(cid) => self.request_fetch(cid, None),
            NetworkCommand::FetchWithReply(cid, reply) => self.request_fetch(cid, Some(reply)),
            NetworkCommand::AddPeer(peer) => {
                println!("PEER_ADDED {}/p2p/{}", peer.address, peer.peer_id);
                self.add_bootstrap(peer);
            }
            // Without the persistent removal Bitswap would still serve the block, so the DHT stays unchanged.
            NetworkCommand::StopProviding(cid) => {
                if !self.forget_provide(&cid) {
                    eprintln!("DHT_STOP_FAILED block is still shared");
                    return;
                }
                match self.stop_providing(&cid) {
                    Ok(()) => println!("DHT_STOPPED {}", cid.to_string_base32()),
                    Err(error) => eprintln!("DHT_STOP_FAILED {error}"),
                }
            }
            NetworkCommand::Bootstrap => {
                self.dht_bootstrap_started = false;
                self.start_dht_bootstrap();
            }
            NetworkCommand::ListPeers => {
                for peer in self.dht_peers() {
                    println!("DHT_PEER {peer}");
                }
                println!("DHT_PEERS_DONE");
            }
            NetworkCommand::DhtServers(reply) => {
                let _ = reply.send(self.dht_peers());
            }
            NetworkCommand::SetDhtMode(mode) => {
                self.swarm
                    .behaviour_mut()
                    .kad
                    .set_mode(Some(mode.libp2p_mode()));
                println!("DHT_MODE {mode:?}");
            }
            NetworkCommand::Quit => {}
        }
    }
    fn request_fetch(&mut self, cid: Cid, reply: Option<FetchReply>) {
        match self.get_providers(&cid) {
            Ok(id) => {
                println!("BITSWAP_FETCH_REQUESTED {}", cid.to_string_base32());
                self.pending_fetches.insert(id, (cid, reply));
            }
            Err(error) => {
                eprintln!("BITSWAP_FETCH_FAILED {error}");
                if let Some(reply) = reply {
                    let _ = reply.send(Err(error.to_string()));
                }
            }
        }
    }
}

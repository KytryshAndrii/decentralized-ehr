use libp2p::swarm::NetworkBehaviour as Libp2pNetworkBehaviour;
use libp2p::{identify, kad, ping};

#[derive(Libp2pNetworkBehaviour)]
#[behaviour(to_swarm = "NetworkEvent")]
pub struct NetworkBehaviour {
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub kad: crate::dht::KademliaBehaviour,
}
#[derive(Debug)]
pub enum NetworkEvent {
    Identify(Box<identify::Event>),
    Ping(ping::Event),
    Kad(Box<kad::Event>),
}

impl From<identify::Event> for NetworkEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(Box::new(event))
    }
}

impl From<ping::Event> for NetworkEvent {
    fn from(event: ping::Event) -> Self {
        Self::Ping(event)
    }
}

impl From<kad::Event> for NetworkEvent {
    fn from(event: kad::Event) -> Self {
        Self::Kad(Box::new(event))
    }
}

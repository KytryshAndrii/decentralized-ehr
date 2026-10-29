use libp2p::{PeerId, identity::Keypair};

pub fn peer_id_from_keypair(keypair: &Keypair) -> PeerId {
    PeerId::from(keypair.public())
}

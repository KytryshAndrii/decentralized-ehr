use libp2p::{PeerId, identity::Keypair};
use std::error::Error;

pub fn peer_id_from_keypair(keypair: &Keypair) -> PeerId {
    PeerId::from(keypair.public())
}

/// Renders an error together with all its sources.
/// libp2p `TransportError::Other` has an empty `Display`; the real cause
/// (e.g. the OS bind error) is only available through `source()`.
pub(crate) fn error_chain(error: &dyn Error) -> String {
    let mut parts = Vec::new();
    let mut current = Some(error);
    while let Some(err) = current {
        let text = err.to_string();
        if !text.is_empty() {
            parts.push(text);
        }
        current = err.source();
    }
    if parts.is_empty() {
        format!("{error:?}")
    } else {
        parts.join(": ")
    }
}

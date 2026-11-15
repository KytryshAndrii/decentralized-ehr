use crate::models::NetworkNode;
use ipfs_bitswap::{BITSWAP_PROTOCOL, BitswapServer, ProvidedBlockSource};

impl NetworkNode {
    /// Uruchamia serwer Bitswap na wspólnym Swarmie. Bloki pochodzą z katalogu stanu,
    /// więc węzeł bez `state_directory` niczego nie udostępnia.
    pub(crate) fn start_bitswap(&mut self) {
        let Some(directory) = self.config.state_directory.clone() else {
            println!("BITSWAP_DISABLED no state directory");
            return;
        };
        let server = BitswapServer::new(ProvidedBlockSource::new(directory));
        match server.start(&self.swarm.behaviour().bitswap) {
            Ok(()) => println!("BITSWAP_LISTENING {BITSWAP_PROTOCOL}"),
            Err(error) => eprintln!("BITSWAP_FAILED {error}"),
        }
    }
}

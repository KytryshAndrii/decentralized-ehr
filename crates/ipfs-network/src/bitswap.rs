use crate::models::NetworkNode;
use ipfs_bitswap::{BITSWAP_PROTOCOL, BitswapClient, BitswapServer, ProvidedBlockSource};
use ipfs_cid::models::Cid;
use libp2p::PeerId;

impl NetworkNode {
    /// Uruchamia serwer i klienta Bitswap na wspólnym Swarmie. Bloki leżą w katalogu stanu,
    /// więc węzeł bez `state_directory` niczego nie udostępnia ani nie pobiera.
    pub(crate) fn start_bitswap(&mut self) {
        let Some(directory) = self.config.state_directory.clone() else {
            println!("BITSWAP_DISABLED no state directory");
            return;
        };
        let behaviour = &self.swarm.behaviour().bitswap;
        let client = BitswapClient::new(behaviour, directory.clone());
        let server = BitswapServer::new(ProvidedBlockSource::new(directory));
        match server.start(behaviour, &client) {
            Ok(()) => {
                println!("BITSWAP_LISTENING {BITSWAP_PROTOCOL}");
                self.bitswap_client = Some(client);
            }
            Err(error) => eprintln!("BITSWAP_FAILED {error}"),
        }
    }

    /// Pobieranie działa w tle, żeby nie blokować pętli zdarzeń węzła.
    pub(crate) fn fetch_block(&self, cid: Cid, providers: Vec<PeerId>) {
        let name = cid.to_string_base32();
        let Some(client) = self.bitswap_client.clone() else {
            eprintln!("BITSWAP_FETCH_FAILED {name}: Bitswap is disabled");
            return;
        };
        tokio::spawn(async move {
            match client.fetch(&cid, &providers).await {
                Ok((peer, bytes)) => println!("BITSWAP_BLOCK_RECEIVED {peer} {name} bytes={bytes}"),
                Err(error) => eprintln!("BITSWAP_FETCH_FAILED {name}: {error}"),
            }
        });
    }
}

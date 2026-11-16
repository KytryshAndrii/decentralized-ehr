use crate::models::NetworkNode;
use ipfs_bitswap::{BITSWAP_PROTOCOL, BitswapClient, BitswapServer, ProvidedBlockSource};
use ipfs_cid::models::Cid;
use libp2p::PeerId;
use std::collections::HashSet;

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

    /// Pobiera blok i wszystkie bloki, do których prowadzą jego linki (cały plik UnixFS).
    /// Działa w tle, żeby nie blokować pętli zdarzeń węzła.
    pub(crate) fn fetch_block(&self, cid: Cid, providers: Vec<PeerId>) {
        let name = cid.to_string_base32();
        let Some(client) = self.bitswap_client.clone() else {
            eprintln!("BITSWAP_FETCH_FAILED {name}: Bitswap is disabled");
            return;
        };
        tokio::spawn(async move {
            match fetch_dag(&client, cid, &providers).await {
                Ok((fetched, local)) => {
                    println!("BITSWAP_FETCH_DONE {name} fetched={fetched} local={local}")
                }
                Err(error) => eprintln!("BITSWAP_FETCH_FAILED {name}: {error}"),
            }
        });
    }
}

/// Zwraca liczbę bloków pobranych z sieci i liczbę bloków, które już były lokalnie.
// shortcut: bloki są pobierane po jednym, każdy osobnym strumieniem; przy dużych plikach
// przejść na wantlistę z wieloma wpisami i pobieranie równoległe.
async fn fetch_dag(
    client: &BitswapClient,
    root: Cid,
    providers: &[PeerId],
) -> Result<(usize, usize), Box<dyn std::error::Error + Send + Sync>> {
    let (mut fetched, mut local) = (0, 0);
    let mut seen = HashSet::new();
    let mut pending = vec![root];
    while let Some(cid) = pending.pop() {
        // Ten sam blok może wystąpić w pliku wiele razy; wystarczy go mieć raz.
        if !seen.insert(cid.to_bytes()) {
            continue;
        }
        let (peer, block) = client.fetch(&cid, providers).await?;
        match peer {
            Some(peer) => {
                fetched += 1;
                println!(
                    "BITSWAP_BLOCK_RECEIVED {peer} {} bytes={}",
                    cid.to_string_base32(),
                    block.len()
                );
            }
            None => local += 1,
        }
        pending.extend(ipfs_unixfs::block_links(&cid, &block)?);
    }
    Ok((fetched, local))
}

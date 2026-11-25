use crate::models::{FetchReply, NetworkNode};
use ipfs_bitswap::{
    BITSWAP_PROTOCOL, BitswapClient, BitswapServer, FETCH_WINDOW, ProvidedBlockSource,
};
use ipfs_cid::models::Cid;
use libp2p::PeerId;
use std::collections::{HashSet, VecDeque};

impl NetworkNode {
    /// Starts the Bitswap server and client on the shared Swarm. Blocks live in the state folder,
    /// so a node without `state_directory` neither serves nor fetches anything.
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

    /// Fetches the block and every block its links lead to (a whole UnixFS file),
    /// in the background so the node event loop is not blocked.
    pub(crate) fn fetch_block(&self, cid: Cid, providers: Vec<PeerId>, reply: Option<FetchReply>) {
        let name = cid.to_string_base32();
        let Some(client) = self.bitswap_client.clone() else {
            eprintln!("BITSWAP_FETCH_FAILED {name}: Bitswap is disabled");
            if let Some(reply) = reply {
                let _ = reply.send(Err("Bitswap is disabled".into()));
            }
            return;
        };
        tokio::spawn(async move {
            let result = match fetch_dag(&client, cid, &providers).await {
                Ok((fetched, local)) => {
                    println!("BITSWAP_FETCH_DONE {name} fetched={fetched} local={local}");
                    Ok(())
                }
                Err(error) => {
                    eprintln!("BITSWAP_FETCH_FAILED {name}: {error}");
                    Err(error.to_string())
                }
            };
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        });
    }
}

/// Returns the number of blocks fetched from the network and of blocks already local.
/// Blocks go in waves of `FETCH_WINDOW`: one wantlist and one database open per wave,
/// and the blocks of a wave are fetched in parallel.
async fn fetch_dag(
    client: &BitswapClient,
    root: Cid,
    providers: &[PeerId],
) -> Result<(usize, usize), Box<dyn std::error::Error + Send + Sync>> {
    let (mut fetched, mut local) = (0, 0);
    let mut seen = HashSet::new();
    let mut pending = VecDeque::from([root]);
    while !pending.is_empty() {
        let mut wave = Vec::new();
        while wave.len() < FETCH_WINDOW
            && let Some(cid) = pending.pop_front()
        {
            // The same block can occur many times in a file; having it once is enough.
            if seen.insert(cid.to_bytes()) {
                wave.push(cid);
            }
        }
        for (cid, (peer, block)) in wave.iter().zip(client.fetch_many(&wave, providers).await?) {
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
            pending.extend(ipfs_unixfs::block_links(cid, &block)?);
        }
    }
    Ok((fetched, local))
}

use crate::codec::write_message;
use crate::consts::{
    BITSWAP_PROTOCOL, FETCH_RESPONSE_TIMEOUT_SECS, FETCH_WINDOW, MAX_MESSAGE_BYTES,
    STREAM_WRITE_TIMEOUT_SECS, WANT_PRIORITY,
};
use crate::error::BitswapError;
use crate::models::{
    BitswapBehaviour, BitswapClient, BitswapMessage, BlockPresenceType, PendingWants, WantMap,
    WantType, Wantlist, WantlistEntry,
};
use crate::service::blocking;
use crate::utils::block_cid;
use futures::AsyncWriteExt;
use futures::channel::oneshot;
use ipfs_cid::models::Cid;
use ipfs_store::PersistentBlockstore;
use libp2p::{PeerId, StreamProtocol};
use std::path::PathBuf;
use std::time::Duration;

impl BitswapClient {
    /// `directory` is the Blockstore folder that receives the fetched blocks.
    pub fn new(behaviour: &BitswapBehaviour, directory: impl Into<PathBuf>) -> Self {
        Self {
            control: behaviour.new_control(),
            directory: directory.into(),
            pending: PendingWants::default(),
        }
    }

    /// A single block; shorthand for `fetch_many`.
    pub async fn fetch(
        &self,
        cid: &Cid,
        providers: &[PeerId],
    ) -> Result<(Option<PeerId>, Vec<u8>), BitswapError> {
        let mut blocks = self
            .fetch_many(std::slice::from_ref(cid), providers)
            .await?;
        Ok(blocks.remove(0))
    }

    /// Returns the blocks (in `cids` order, no repeats): local ones from the Blockstore, missing
    /// ones fetched from providers with one wantlist per provider, checked against the CID and stored.
    /// The database is opened once for reading and once for writing, whatever the block count.
    /// Provider `None` means a local block. When a block cannot be fetched, the other fetched
    /// blocks are still stored (a retry does not start from zero).
    pub async fn fetch_many(
        &self,
        cids: &[Cid],
        providers: &[PeerId],
    ) -> Result<Vec<(Option<PeerId>, Vec<u8>)>, BitswapError> {
        let (directory, wanted) = (self.directory.clone(), cids.to_vec());
        let local = blocking(move || {
            let store = PersistentBlockstore::open_in(&directory)?;
            wanted
                .iter()
                .map(|cid| Ok(store.get_block(cid)?))
                .collect::<Result<Vec<_>, BitswapError>>()
        })
        .await?;
        let mut results: Vec<Option<(Option<PeerId>, Vec<u8>)>> = vec![None; cids.len()];
        let mut missing = Vec::new();
        for (index, block) in local.into_iter().enumerate() {
            match block {
                Some(block) => results[index] = Some((None, block)),
                None => missing.push(index),
            }
        }
        let mut last_error = BitswapError::NoProviders;
        let mut received: Vec<(usize, Vec<u8>)> = Vec::new();
        for peer in providers {
            if missing.is_empty() {
                break;
            }
            let wanted: Vec<(usize, &Cid)> = missing.iter().map(|&i| (i, &cids[i])).collect();
            let (got, error) = self.fetch_batch_from(*peer, &wanted).await;
            if let Some(error) = error {
                eprintln!("BITSWAP_PROVIDER_FAILED {peer}: {error}");
                last_error = error;
            }
            for (index, data) in got {
                results[index] = Some((Some(*peer), data.clone()));
                received.push((index, data));
            }
            missing.retain(|&i| results[i].is_none());
        }
        if !received.is_empty() {
            let directory = self.directory.clone();
            let to_store: Vec<(Cid, Vec<u8>)> = received
                .into_iter()
                .map(|(index, data)| (cids[index].clone(), data))
                .collect();
            // put_block hashes again: only a block matching its CID is stored.
            blocking(move || {
                let store = PersistentBlockstore::open_in(&directory)?;
                to_store
                    .iter()
                    .try_for_each(|(cid, data)| Ok(store.put_block(cid, data)?))
            })
            .await?;
        }
        if !missing.is_empty() {
            return Err(last_error);
        }
        Ok(results.into_iter().flatten().collect())
    }

    /// One wantlist (in batches of `FETCH_WINDOW` entries) to one provider.
    /// Returns the blocks that arrived and the last error (DontHave, Timeout, ...).
    async fn fetch_batch_from(
        &self,
        peer: PeerId,
        wanted: &[(usize, &Cid)],
    ) -> (Vec<(usize, Vec<u8>)>, Option<BitswapError>) {
        let mut error = None;
        let mut waiting = Vec::new();
        {
            let mut wants = self.wants();
            for &(index, cid) in wanted {
                let key = (peer, cid.to_bytes());
                // A second identical request would overwrite the first sender and both would be lost.
                if wants.contains_key(&key) {
                    error = Some(BitswapError::Stream(
                        "block is already being fetched".into(),
                    ));
                    continue;
                }
                let (sender, receiver) = oneshot::channel();
                wants.insert(key.clone(), sender);
                waiting.push((index, cid, key, receiver));
            }
        }
        let cids: Vec<&Cid> = waiting.iter().map(|(_, cid, ..)| *cid).collect();
        let mut sent = Ok(());
        for window in cids.chunks(FETCH_WINDOW) {
            sent = self.send_wants(peer, window, false).await;
            if sent.is_err() {
                break;
            }
        }
        let mut got = Vec::new();
        let mut to_cancel: Vec<&Cid> = Vec::new();
        for (index, cid, key, receiver) in waiting {
            // Waiting in order: the timeout counts from the previous block and blocks that already
            // arrived finish at once, so a slow link does not end in a false Timeout.
            let reply = match &sent {
                Ok(()) => {
                    tokio::time::timeout(Duration::from_secs(FETCH_RESPONSE_TIMEOUT_SECS), receiver)
                        .await
                        .map_err(|_| BitswapError::Timeout)
                        .and_then(|reply| {
                            reply.map_err(|_| BitswapError::Stream("want dropped".into()))
                        })
                }
                Err(_) => Err(BitswapError::Stream("want not sent".into())),
            };
            self.wants().remove(&key);
            match reply {
                Ok(Some(data)) => got.push((index, data)),
                // Kubo keeps the want after DontHave and Timeout too, so it must be cancelled.
                Ok(None) => {
                    to_cancel.push(cid);
                    error = Some(BitswapError::DontHave);
                }
                Err(failure) => {
                    to_cancel.push(cid);
                    error = Some(failure);
                }
            }
        }
        if let Err(failure) = sent {
            error = Some(failure);
        }
        for window in to_cancel.chunks(FETCH_WINDOW) {
            let _ = self.send_wants(peer, window, true).await;
        }
        (got, error)
    }

    fn wants(&self) -> std::sync::MutexGuard<'_, WantMap> {
        lock(&self.pending)
    }

    /// One message with Want-Block entries or their cancellation.
    async fn send_wants(
        &self,
        peer: PeerId,
        cids: &[&Cid],
        cancel: bool,
    ) -> Result<(), BitswapError> {
        let message = BitswapMessage {
            wantlist: Some(Wantlist {
                entries: cids
                    .iter()
                    .map(|cid| WantlistEntry {
                        block: cid.to_bytes(),
                        priority: WANT_PRIORITY,
                        cancel,
                        want_type: WantType::Block as i32,
                        send_dont_have: !cancel,
                    })
                    .collect(),
                full: false,
            }),
            ..BitswapMessage::default()
        };
        let mut control = self.control.clone();
        let send = async {
            let mut stream = control
                .open_stream(peer, StreamProtocol::new(BITSWAP_PROTOCOL))
                .await
                .map_err(|error| BitswapError::Stream(error.to_string()))?;
            write_message(&mut stream, &message, MAX_MESSAGE_BYTES).await?;
            Ok(stream.close().await?)
        };
        tokio::time::timeout(Duration::from_secs(STREAM_WRITE_TIMEOUT_SECS), send)
            .await
            .map_err(|_| BitswapError::Timeout)?
    }
}

/// Hands the blocks and DontHave of a received message to the waiting requests.
/// The key is the CID computed from the received bytes, so a corrupted block or one not
/// requested from this peer matches no request and is dropped.
pub(crate) fn deliver(pending: &PendingWants, peer: PeerId, message: &mut BitswapMessage) {
    let mut wants = lock(pending);
    for block in std::mem::take(&mut message.payload) {
        let sender = block_cid(&block.prefix, &block.data)
            .and_then(|cid| wants.remove(&(peer, cid.to_bytes())));
        match sender {
            Some(sender) => drop(sender.send(Some(block.data))),
            None => eprintln!(
                "BITSWAP_BLOCK_REJECTED {peer} bytes={} (not requested or does not match CID)",
                block.data.len()
            ),
        }
    }
    for presence in &message.block_presences {
        if presence.r#type() == BlockPresenceType::DontHave
            && let Some(sender) = wants.remove(&(peer, presence.cid.clone()))
        {
            drop(sender.send(None));
        }
    }
}

/// The map has no invariants between entries, so a poisoned mutex is still usable.
fn lock(pending: &PendingWants) -> std::sync::MutexGuard<'_, WantMap> {
    pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

use crate::codec::write_message;
use crate::consts::{
    BITSWAP_PROTOCOL, FETCH_RESPONSE_TIMEOUT_SECS, MAX_MESSAGE_BYTES, STREAM_WRITE_TIMEOUT_SECS,
    WANT_PRIORITY,
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
use std::collections::hash_map::Entry;
use std::path::PathBuf;
use std::time::Duration;

impl BitswapClient {
    /// `directory` to katalog Blockstore, do którego trafiają pobrane bloki.
    pub fn new(behaviour: &BitswapBehaviour, directory: impl Into<PathBuf>) -> Self {
        Self {
            control: behaviour.new_control(),
            directory: directory.into(),
            pending: PendingWants::default(),
        }
    }

    /// Zwraca blok z lokalnego Blockstore albo pobiera go od pierwszego dostawcy, który go
    /// wyda, weryfikuje względem CID i zapisuje. Dostawca `None` oznacza blok lokalny.
    pub async fn fetch(
        &self,
        cid: &Cid,
        providers: &[PeerId],
    ) -> Result<(Option<PeerId>, Vec<u8>), BitswapError> {
        let (directory, wanted) = (self.directory.clone(), cid.clone());
        let local =
            blocking(move || Ok(PersistentBlockstore::open_in(&directory)?.get_block(&wanted)?));
        if let Some(block) = local.await? {
            return Ok((None, block));
        }
        let mut last_error = BitswapError::NoProviders;
        for peer in providers {
            match self.fetch_from(*peer, cid).await {
                Ok(data) => {
                    let (directory, cid) = (self.directory.clone(), cid.clone());
                    // put_block ponownie liczy skrót: do bazy trafia tylko blok zgodny z CID.
                    let data = blocking(move || {
                        PersistentBlockstore::open_in(&directory)?.put_block(&cid, &data)?;
                        Ok(data)
                    })
                    .await?;
                    return Ok((Some(*peer), data));
                }
                Err(error) => {
                    eprintln!("BITSWAP_PROVIDER_FAILED {peer}: {error}");
                    last_error = error;
                }
            }
        }
        Err(last_error)
    }

    async fn fetch_from(&self, peer: PeerId, cid: &Cid) -> Result<Vec<u8>, BitswapError> {
        let key = (peer, cid.to_bytes());
        let (sender, receiver) = oneshot::channel();
        // Drugie takie samo żądanie nadpisałoby nadawcę pierwszego i oba by przepadły.
        match self.wants().entry(key.clone()) {
            Entry::Occupied(_) => {
                return Err(BitswapError::Stream(
                    "block is already being fetched".into(),
                ));
            }
            Entry::Vacant(slot) => slot.insert(sender),
        };
        let sent = self.send_want(peer, cid, false).await;
        let reply = match sent {
            Ok(()) => {
                tokio::time::timeout(Duration::from_secs(FETCH_RESPONSE_TIMEOUT_SECS), receiver)
                    .await
                    .map_err(|_| BitswapError::Timeout)
                    .and_then(|reply| {
                        reply.map_err(|_| BitswapError::Stream("want dropped".into()))
                    })
            }
            Err(error) => Err(error),
        };
        self.wants().remove(&key);
        match reply {
            Ok(Some(data)) => Ok(data),
            Ok(None) => {
                // Kubo pamięta want także po DontHave, więc trzeba go wycofać.
                let _ = self.send_want(peer, cid, true).await;
                Err(BitswapError::DontHave)
            }
            Err(BitswapError::Timeout) => {
                let _ = self.send_want(peer, cid, true).await;
                Err(BitswapError::Timeout)
            }
            Err(error) => Err(error),
        }
    }

    fn wants(&self) -> std::sync::MutexGuard<'_, WantMap> {
        lock(&self.pending)
    }

    /// Jedna wiadomość z jednym wpisem Want-Block albo jego anulowaniem.
    async fn send_want(&self, peer: PeerId, cid: &Cid, cancel: bool) -> Result<(), BitswapError> {
        let message = BitswapMessage {
            wantlist: Some(Wantlist {
                entries: vec![WantlistEntry {
                    block: cid.to_bytes(),
                    priority: WANT_PRIORITY,
                    cancel,
                    want_type: WantType::Block as i32,
                    send_dont_have: !cancel,
                }],
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

/// Przekazuje bloki i DontHave z odebranej wiadomości czekającym żądaniom.
/// Kluczem jest CID wyliczony z odebranych bajtów, więc blok uszkodzony
/// albo niezamówiony u tego peera nie pasuje do żadnego żądania i jest odrzucany.
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

/// Mapa nie ma niezmienników między wpisami, więc zatruty mutex jest nadal użyteczny.
fn lock(pending: &PendingWants) -> std::sync::MutexGuard<'_, WantMap> {
    pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

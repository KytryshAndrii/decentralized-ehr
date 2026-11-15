use crate::client::deliver;
use crate::codec::{read_message, write_message};
use crate::consts::{BITSWAP_PROTOCOL, STREAM_WRITE_TIMEOUT_SECS};
use crate::error::BitswapError;
use crate::models::{
    BitswapBehaviour, BitswapClient, BitswapMessage, BitswapServer, BlockPresenceType,
    PendingWants, ResponsePlan,
};
use crate::source::BlockSource;
use futures::{AsyncWriteExt, StreamExt};
use libp2p::{PeerId, Stream, StreamProtocol};
use libp2p_stream::Control;
use std::sync::Arc;
use std::time::Duration;

pub fn new_behaviour() -> BitswapBehaviour {
    BitswapBehaviour::new()
}

impl<S: BlockSource> BitswapServer<S> {
    /// Rejestruje protokół i uruchamia obsługę strumieni w tle (wymaga runtime tokio).
    /// Protokół ma jeden odbiornik strumieni, więc odpowiedzi dla `client` też przechodzą tędy.
    pub fn start(
        self,
        behaviour: &BitswapBehaviour,
        client: &BitswapClient,
    ) -> Result<(), BitswapError> {
        let pending = client.pending.clone();
        let mut control = behaviour.new_control();
        let mut incoming = control
            .accept(StreamProtocol::new(BITSWAP_PROTOCOL))
            .map_err(|error| BitswapError::Stream(error.to_string()))?;
        let server = Arc::new(self);
        tokio::spawn(async move {
            while let Some((peer, stream)) = incoming.next().await {
                let server = server.clone();
                tokio::spawn(server.serve_stream(control.clone(), pending.clone(), peer, stream));
            }
        });
        Ok(())
    }

    /// Jeden strumień przychodzący może nieść wiele wiadomości z wantlistą.
    async fn serve_stream(
        self: Arc<Self>,
        mut control: Control,
        pending: PendingWants,
        peer: PeerId,
        mut stream: Stream,
    ) {
        loop {
            match read_message(&mut stream, self.limits.max_message_bytes).await {
                Ok(Some(mut request)) => {
                    deliver(&pending, peer, &mut request);
                    if let Err(error) = self.clone().answer(&mut control, peer, request).await {
                        eprintln!("BITSWAP_FAILED {peer}: {error}");
                    }
                }
                Ok(None) => return,
                Err(error) => {
                    eprintln!("BITSWAP_FAILED {peer}: {error}");
                    return;
                }
            }
        }
    }

    /// Odpowiedź idzie nowym strumieniem otwartym przez ten węzeł, jak wymaga specyfikacja.
    async fn answer(
        self: Arc<Self>,
        control: &mut Control,
        peer: PeerId,
        request: BitswapMessage,
    ) -> Result<(), BitswapError> {
        let entries = request.wantlist.as_ref().map_or(0, |w| w.entries.len());
        if entries == 0 {
            return Ok(());
        }
        println!("BITSWAP_REQUEST {peer} entries={entries}");
        let server = self.clone();
        let mut plan = blocking(move || server.plan(&request)).await?;
        if plan.ignored_entries > 0 {
            eprintln!(
                "BITSWAP_WANTLIST_TRUNCATED {peer} ignored={}",
                plan.ignored_entries
            );
        }
        if plan.is_empty() {
            return Ok(());
        }
        let mut stream = control
            .open_stream(peer, StreamProtocol::new(BITSWAP_PROTOCOL))
            .await
            .map_err(|error| BitswapError::Stream(error.to_string()))?;
        let mut logged = 0;
        loop {
            let server = self.clone();
            let (next, returned) = blocking(move || {
                let next = server.next_message(&mut plan);
                Ok((next, plan))
            })
            .await?;
            plan = returned;
            let Some(message) = next? else { break };
            let write = write_message(&mut stream, &message, self.limits.max_message_bytes);
            tokio::time::timeout(Duration::from_secs(STREAM_WRITE_TIMEOUT_SECS), write)
                .await
                .map_err(|_| BitswapError::Timeout)??;
            log_sent(peer, &message, &plan, &mut logged);
        }
        if plan.dropped_blocks > 0 {
            eprintln!(
                "BITSWAP_RESPONSE_TRUNCATED {peer} dropped={}",
                plan.dropped_blocks
            );
        }
        stream.close().await?;
        Ok(())
    }
}

/// Dostęp do LevelDB jest blokujący, więc nie może działać w wątku pętli sieciowej.
pub(crate) async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, BitswapError> + Send + 'static,
) -> Result<T, BitswapError> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| BitswapError::Stream(error.to_string()))?
}

fn log_sent(peer: PeerId, message: &BitswapMessage, plan: &ResponsePlan, logged: &mut usize) {
    let have = message
        .block_presences
        .iter()
        .filter(|presence| presence.r#type() == BlockPresenceType::Have)
        .count();
    let dont_have = message.block_presences.len() - have;
    if have + dont_have > 0 {
        println!("BITSWAP_PRESENCE_SENT {peer} have={have} dont_have={dont_have}");
    }
    for (cid, block) in plan.sent_blocks[*logged..].iter().zip(&message.payload) {
        println!(
            "BITSWAP_BLOCK_SENT {peer} {} bytes={}",
            cid.to_string_base32(),
            block.data.len()
        );
    }
    *logged = plan.sent_blocks.len();
}

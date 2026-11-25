//! Replication `/ehr/replicate/1.0.0`: after issuing, the doctor asks another doctor's node (a DHT
//! server) to fetch, pin and provide the document. With the doctor and the patient (who pins its
//! own documents) this gives 3 copies. Not watched later.
use crate::consts::{
    REPLICATE_MAX_CIDS, REPLICATE_PROTOCOL, REPLICATE_TIMEOUT_SECS, REQUEST_MAX_AGE_SECS,
};
use crate::error::EhrError;
use crate::models::{EhrNode, ReplicateRequest, StreamReply};
use crate::node::network;
use crate::utils::utc_now_secs;
use crate::wire::{read_frame, write_frame};
use ehr_security::{Signed, peer_id_from_did};
use ipfs_network::NetworkCommand;
use libp2p::{PeerId, Stream};
use std::time::Duration;
use tokio::sync::oneshot;

impl EhrNode {
    /// Runs in the background: DHT servers are asked one by one until one keeps a copy; nodes
    /// without the protocol (patients, Kubo) refuse it.
    pub(crate) fn replicate(&self, cids: Vec<String>, patient: PeerId) {
        let node = self.clone();
        tokio::spawn(async move {
            let (reply, servers) = oneshot::channel();
            if node.send(NetworkCommand::DhtServers(reply)).is_err() {
                return;
            }
            let Ok(servers) = servers.await else { return };
            for peer in servers.into_iter().filter(|peer| *peer != patient) {
                match node.ask_replica(peer, &cids).await {
                    Ok(()) => return println!("[REPLICA] STORED peer={peer}"),
                    Err(error) => eprintln!("[REPLICA_FAILED] {peer}: {error}"),
                }
            }
            println!("[REPLICA] NONE");
        });
    }

    async fn ask_replica(&self, peer: PeerId, cids: &[String]) -> Result<(), EhrError> {
        // One attempt per peer: a server without the EHR program (e.g. Kubo) refuses the protocol.
        let mut stream = self
            .control
            .clone()
            .open_stream(peer, REPLICATE_PROTOCOL)
            .await
            .map_err(network)?;
        let request = ReplicateRequest {
            cids: cids.to_vec(),
            created_utc: utc_now_secs(),
        };
        write_frame(
            &mut stream,
            &Signed::sign(&self.app.identity.keypair, &request)?,
        )
        .await?;
        let timeout = Duration::from_secs(REPLICATE_TIMEOUT_SECS);
        let signed: Signed = tokio::time::timeout(timeout, read_frame(&mut stream))
            .await
            .map_err(|_| EhrError::Network("replica timed out".into()))??;
        let reply: StreamReply = signed.open()?;
        if peer_id_from_did(&signed.signer)? != peer {
            return Err(EhrError::Rejected("reply not signed by the replica".into()));
        }
        if !reply.accepted {
            return Err(EhrError::Rejected(reply.reason));
        }
        Ok(())
    }

    pub(crate) async fn answer_replicate(
        &self,
        peer: PeerId,
        mut stream: Stream,
    ) -> Result<(), EhrError> {
        let signed: Signed = read_frame(&mut stream).await?;
        let request: ReplicateRequest = signed.open()?;
        if peer_id_from_did(&signed.signer)? != peer {
            return Err(EhrError::Rejected(
                "request not signed by the sender".into(),
            ));
        }
        if utc_now_secs().abs_diff(request.created_utc) > REQUEST_MAX_AGE_SECS {
            return Err(EhrError::Rejected("stale request".into()));
        }
        if request.cids.is_empty() || request.cids.len() > REPLICATE_MAX_CIDS {
            return Err(EhrError::Rejected(
                "expected a document and its manifest".into(),
            ));
        }
        let stored = self.keep_copy(&request.cids).await;
        let reply = StreamReply {
            accepted: stored.is_ok(),
            reason: match &stored {
                Ok(()) => "stored".into(),
                Err(error) => error.to_string(),
            },
        };
        self.reply_signed(&mut stream, &reply).await?;
        stored
    }

    /// The replica only holds ciphertext; `fetch` pins and provides it.
    async fn keep_copy(&self, cids: &[String]) -> Result<(), EhrError> {
        for cid in cids {
            self.fetch(cid).await?;
        }
        println!("[REPLICA] KEPT count={}", cids.len());
        Ok(())
    }
}

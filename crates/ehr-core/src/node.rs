//! EHR network operations on a running IPFS node, shared by both roles. Role-specific parts live in
//! `doctor.rs`, `patient.rs` and `replica.rs`. The node is the only owner of the blockstore in the
//! process; the `ipfs-*` crates only ever see ciphertext (encrypt-then-hash).
use crate::consts::{
    CONTACT_PROTOCOL, ENV_PUBLIC_IP, INBOX_PROTOCOL, REPLICATE_PROTOCOL, REPLICATE_TIMEOUT_SECS,
    STREAM_TIMEOUT_SECS,
};
use crate::error::EhrError;
use crate::models::{Contact, EhrApp, EhrNode, Role};
use crate::store::ContactKind;
use crate::utils::{patient_dht_server, read_battery};
use crate::wire::write_frame;
use ehr_security::{ContactCard, Signed};
use futures::AsyncWriteExt;
use futures::StreamExt;
use ipfs_cid::models::Cid;
use ipfs_network::{
    BootstrapPeer, DhtMode, NetworkCommand, NetworkConfig, NetworkNode, StreamControl,
};
use ipfs_store::PersistentBlockstore;
use libp2p::{Multiaddr, PeerId, Stream, StreamProtocol};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Blocking work (LevelDB, encryption, UnixFS) runs off the event loop threads.
pub(crate) async fn blocking<T: Send + 'static>(
    app: &Arc<EhrApp>,
    work: impl FnOnce(&EhrApp) -> Result<T, EhrError> + Send + 'static,
) -> Result<T, EhrError> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || work(&app))
        .await
        .map_err(|error| EhrError::Network(error.to_string()))?
}

pub(crate) fn network(error: impl std::fmt::Display) -> EhrError {
    EhrError::Network(error.to_string())
}

impl EhrApp {
    /// Starts the IPFS node with the key unlocked by the password. The doctor (= facility) is
    /// a DHT server and serves the inbox and invitations; the patient connects to the doctors
    /// from its contacts, polls them for new documents and keeps its storage under the limit.
    pub async fn start_node(
        self: Arc<Self>,
        listen: Multiaddr,
        mut bootstrap: Vec<BootstrapPeer>,
        dht_server: bool,
    ) -> Result<(EhrNode, JoinHandle<Result<(), String>>), EhrError> {
        let dht_server_requested = dht_server;
        if self.role == Role::Patient {
            for doctor in self.contacts(ContactKind::Doctor)? {
                if let Some(peer) = doctor_address(&doctor)?
                    && !bootstrap.iter().any(|known| known.peer_id == peer.peer_id)
                {
                    bootstrap.push(peer);
                }
            }
        }
        // The doctor is always a DHT server; a patient asking for it also needs enough battery.
        let dht_server = match self.role {
            Role::Doctor => true,
            Role::Patient => dht_server && patient_dht_server(read_battery(), true),
        };
        let watch_battery = self.role == Role::Patient && dht_server_requested;
        let config = NetworkConfig {
            listen,
            state_directory: Some(self.data_dir.clone()),
            keypair: Some(self.identity.keypair.clone()),
            dht_mode: if dht_server {
                DhtMode::Server
            } else {
                DhtMode::Client
            },
            bootstrap,
            ..NetworkConfig::default()
        };
        let node = NetworkNode::new_network_node(config).map_err(network)?;
        let (commands, receiver) = mpsc::unbounded_channel();
        let ehr = EhrNode {
            control: node.stream_control(),
            app: self,
            commands,
            dht_server: Arc::new(AtomicBool::new(dht_server)),
        };
        match ehr.app.role {
            Role::Doctor => {
                ehr.serve(INBOX_PROTOCOL)?;
                ehr.serve(CONTACT_PROTOCOL)?;
                // Only facility nodes keep replicas, so nobody can grow a patient's pinned storage.
                ehr.serve(REPLICATE_PROTOCOL)?;
            }
            Role::Patient => {
                ehr.poll_inbox();
                ehr.collect_garbage_periodically();
                if watch_battery {
                    ehr.watch_battery();
                }
            }
        }
        let handle = tokio::spawn(async move {
            node.run_with_commands(receiver)
                .await
                .map_err(|error| error.to_string())
        });
        Ok((ehr, handle))
    }

    /// Node address for an invitation: `EHR_PUBLIC_IP` or the IP of the interface with the
    /// default route (no packet is sent), port taken from the listen address.
    pub fn node_address(listen: &Multiaddr) -> Result<String, EhrError> {
        let ip = match std::env::var(ENV_PUBLIC_IP) {
            Ok(ip) => ip,
            Err(_) => {
                let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
                socket.connect("192.0.2.1:9")?;
                socket.local_addr()?.ip().to_string()
            }
        };
        let port = listen
            .iter()
            .find_map(|protocol| match protocol {
                libp2p::multiaddr::Protocol::Tcp(port) => Some(port),
                _ => None,
            })
            .ok_or_else(|| EhrError::Invalid("listen address without TCP port".into()))?;
        Ok(format!("/ip4/{ip}/tcp/{port}"))
    }
}

fn doctor_address(doctor: &Contact) -> Result<Option<BootstrapPeer>, EhrError> {
    let card = ContactCard::from_text(&doctor.card)?;
    card.payload
        .multiaddr
        .map(|address| BootstrapPeer::parse(&address).map_err(network))
        .transpose()
}

impl EhrNode {
    pub fn app(&self) -> &Arc<EhrApp> {
        &self.app
    }

    /// Current DHT mode; a patient on battery may change it at runtime.
    pub fn dht_server(&self) -> bool {
        self.dht_server.load(Ordering::Relaxed)
    }

    /// Command channel of the IPFS node (CLI: `provide`, `fetch`, `peers`, `quit`, ...).
    pub fn commands(&self) -> &mpsc::UnboundedSender<NetworkCommand> {
        &self.commands
    }

    pub(crate) fn send(&self, command: NetworkCommand) -> Result<(), EhrError> {
        self.commands
            .send(command)
            .map_err(|_| EhrError::Network("node stopped".into()))
    }

    fn serve(&self, protocol: StreamProtocol) -> Result<(), EhrError> {
        let mut incoming = self
            .control
            .clone()
            .accept(protocol.clone())
            .map_err(network)?;
        let node = self.clone();
        tokio::spawn(async move {
            while let Some((peer, stream)) = incoming.next().await {
                let node = node.clone();
                let protocol = protocol.clone();
                tokio::spawn(async move {
                    // A replica downloads the whole document before it answers.
                    let timeout_secs = if protocol == REPLICATE_PROTOCOL {
                        REPLICATE_TIMEOUT_SECS
                    } else {
                        STREAM_TIMEOUT_SECS
                    };
                    let work = async {
                        if protocol == INBOX_PROTOCOL {
                            node.answer_inbox(peer, stream).await
                        } else if protocol == CONTACT_PROTOCOL {
                            node.answer_contact(peer, stream).await
                        } else {
                            node.answer_replicate(peer, stream).await
                        }
                    };
                    let timeout = Duration::from_secs(timeout_secs);
                    match tokio::time::timeout(timeout, work).await {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => eprintln!("[EHR_FAILED] {protocol} {peer}: {error}"),
                        Err(_) => eprintln!("[EHR_FAILED] {protocol} {peer}: timeout"),
                    }
                });
            }
        });
        Ok(())
    }

    /// Stream to a peer; right after `AddPeer` the connection may still be in progress.
    pub(crate) async fn open(
        &self,
        peer: PeerId,
        protocol: StreamProtocol,
    ) -> Result<Stream, EhrError> {
        let mut control: StreamControl = self.control.clone();
        let mut last_error = String::new();
        for _ in 0..STREAM_TIMEOUT_SECS * 2 {
            match control.open_stream(peer, protocol.clone()).await {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = error.to_string(),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(EhrError::Network(format!(
            "{peer} unreachable: {last_error}"
        )))
    }

    /// The whole file locally: from the blockstore or fetched over Bitswap. A downloaded copy is
    /// an EHR record, so it is pinned (the GC removes only the cache); a DHT server announces it.
    pub(crate) async fn fetch(&self, cid: &str) -> Result<Cid, EhrError> {
        let cid = Cid::decode_from_string(cid)?;
        let local = {
            let cid = cid.clone();
            blocking(&self.app, move |app| {
                let store = PersistentBlockstore::open_in(&app.data_dir)?;
                Ok(ipfs_unixfs::dag_cids(&store, &cid).is_ok())
            })
            .await?
        };
        if !local {
            let (reply, result) = oneshot::channel();
            self.send(NetworkCommand::FetchWithReply(cid.clone(), reply))?;
            tokio::time::timeout(Duration::from_secs(STREAM_TIMEOUT_SECS * 4), result)
                .await
                .map_err(|_| EhrError::Network("fetch timed out".into()))?
                .map_err(network)?
                .map_err(EhrError::Network)?;
            let root = cid.clone();
            let cids = blocking(&self.app, move |app| {
                let store = PersistentBlockstore::open_in(&app.data_dir)?;
                let cids = ipfs_unixfs::dag_cids(&store, &root)?;
                for cid in &cids {
                    store.pin(cid)?;
                }
                Ok(cids)
            })
            .await?;
            if self.dht_server.load(Ordering::Relaxed) {
                self.provide(cids)?;
            }
        }
        Ok(cid)
    }

    /// Leaves before roots: once a peer finds the root, every block of the file is served.
    /// Signed answer on an incoming stream, then the stream is closed.
    pub(crate) async fn reply_signed(
        &self,
        stream: &mut Stream,
        value: &impl serde::Serialize,
    ) -> Result<(), EhrError> {
        write_frame(stream, &Signed::sign(&self.app.identity.keypair, value)?).await?;
        Ok(stream.close().await?)
    }

    pub(crate) fn provide(&self, cids: Vec<Cid>) -> Result<(), EhrError> {
        for cid in cids.into_iter().rev() {
            self.send(NetworkCommand::StartProviding(cid))?;
        }
        Ok(())
    }

    pub(crate) async fn read_file(&self, cid: &str) -> Result<Vec<u8>, EhrError> {
        let cid = self.fetch(cid).await?;
        blocking(&self.app, move |app| {
            let store = PersistentBlockstore::open_in(&app.data_dir)?;
            let mut bytes = Vec::new();
            ipfs_unixfs::cat(&store, &cid, &mut bytes)?;
            Ok(bytes)
        })
        .await
    }
}

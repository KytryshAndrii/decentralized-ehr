//! Dodatki dla warstwy EHR (M9): gotowy klucz w konfiguracji, `AddPeer` w trakcie pracy,
//! `FetchWithReply` z wynikiem w kanale i uchwyt strumieni obok Bitswap.
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use ipfs_network::{BootstrapPeer, DhtMode, NetworkCommand, NetworkConfig, NetworkNode};
use ipfs_store::PersistentBlockstore;
use libp2p::{Multiaddr, PeerId, StreamProtocol, identity::Keypair};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

const ECHO: StreamProtocol = StreamProtocol::new("/ehr/test-echo/1.0.0");

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn config(dir: &std::path::Path, port: u16, keypair: Keypair, mode: DhtMode) -> NetworkConfig {
    NetworkConfig {
        listen: format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap(),
        identity_path: dir.join("unused-node-key.pb"),
        state_directory: Some(dir.to_path_buf()),
        keypair: Some(keypair),
        dht_mode: mode,
        ..NetworkConfig::default()
    }
}

async fn fetch(
    sender: &mpsc::UnboundedSender<NetworkCommand>,
    cid: ipfs_cid::models::Cid,
) -> Result<(), String> {
    let (reply, result) = oneshot::channel();
    sender
        .send(NetworkCommand::FetchWithReply(cid, reply))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(60), result)
        .await
        .expect("fetch timed out")
        .expect("reply dropped")
}

#[tokio::test(flavor = "multi_thread")]
async fn added_peer_serves_fetch_with_reply_and_application_streams() {
    let (dir_a, dir_b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (key_a, key_b) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());
    let (id_a, port_a) = (PeerId::from(key_a.public()), free_port());

    let block = b"synthetic lab result".to_vec();
    let cid = PersistentBlockstore::open_in(dir_a.path())
        .unwrap()
        .put(&block)
        .unwrap();
    let missing = ipfs_cid::models::Cid::generate_from_data(
        b"nobody has it",
        ipfs_cid::consts::CODEC_RAW,
        ipfs_cid::consts::SHA2_256,
    )
    .unwrap();

    // Węzeł A: gotowy klucz z konfiguracji, a nie z pliku.
    let node_a =
        NetworkNode::new_network_node(config(dir_a.path(), port_a, key_a, DhtMode::Server))
            .unwrap();
    assert_eq!(node_a.local_peer_id(), id_a);
    assert!(!dir_a.path().join("unused-node-key.pb").exists());
    let mut incoming = node_a.stream_control().accept(ECHO).unwrap();
    tokio::spawn(async move {
        while let Some((_, mut stream)) = incoming.next().await {
            let mut buffer = Vec::new();
            stream.read_to_end(&mut buffer).await.unwrap();
            buffer.reverse();
            stream.write_all(&buffer).await.unwrap();
            stream.close().await.unwrap();
        }
    });
    let (send_a, receive_a) = mpsc::unbounded_channel();
    tokio::spawn(node_a.run_with_commands(receive_a));
    send_a
        .send(NetworkCommand::StartProviding(cid.clone()))
        .unwrap();

    // Węzeł B startuje bez bootstrapu i dostaje adres A dopiero w trakcie pracy.
    let node_b =
        NetworkNode::new_network_node(config(dir_b.path(), free_port(), key_b, DhtMode::Client))
            .unwrap();
    let mut control_b = node_b.stream_control();
    let (send_b, receive_b) = mpsc::unbounded_channel();
    tokio::spawn(node_b.run_with_commands(receive_b));
    let address: Multiaddr = format!("/ip4/127.0.0.1/tcp/{port_a}").parse().unwrap();
    send_b
        .send(NetworkCommand::AddPeer(BootstrapPeer {
            peer_id: id_a,
            address,
        }))
        .unwrap();

    // Strumień własnego protokołu na tym samym połączeniu co Bitswap.
    let mut stream = None;
    for _ in 0..50 {
        if let Ok(opened) = control_b.open_stream(id_a, ECHO).await {
            stream = Some(opened);
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let mut stream = stream.expect("stream to the added peer");
    stream.write_all(b"abc").await.unwrap();
    stream.close().await.unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.unwrap();
    assert_eq!(answer, b"cba");

    assert_eq!(fetch(&send_b, cid.clone()).await, Ok(()));
    let store_b = PersistentBlockstore::open_in(dir_b.path()).unwrap();
    assert_eq!(store_b.get_block(&cid).unwrap(), Some(block));
    drop(store_b);
    assert!(fetch(&send_b, missing).await.is_err());

    send_a.send(NetworkCommand::Quit).unwrap();
    send_b.send(NetworkCommand::Quit).unwrap();
}

//! Integration: a real `ehr-cli` node serves blocks over Bitswap 1.2.0 to a minimal test client,
//! and `ehr-cli` nodes fetch blocks from each other (DHT -> Bitswap -> Blockstore).
//! cargo build -p ehr-cli
//! cargo test -p test-runner --test integration-bitswap -- --nocapture
use futures::{AsyncWriteExt, StreamExt};
use ipfs_bitswap::consts::MAX_MESSAGE_BYTES;
use ipfs_bitswap::{
    BITSWAP_PROTOCOL, BitswapControl, BitswapIncomingStreams, BitswapMessage, BlockPresenceType,
    WantType, Wantlist, WantlistEntry, read_message, write_message,
};
use ipfs_cid::models::Cid;
use ipfs_store::{PersistentBlockstore, consts::DEFAULT_DATA_DIR};
use libp2p::{Multiaddr, PeerId, StreamProtocol, SwarmBuilder};
use std::{fs, path::Path, process::Command, time::Duration};
use test_runner::node_process::{NodeProcess, cli, free_port, put_file};

const WAIT_SECS: u64 = 20;

fn want(cid: &Cid, want_type: WantType) -> WantlistEntry {
    WantlistEntry {
        block: cid.to_bytes(),
        priority: 1,
        cancel: false,
        want_type: want_type as i32,
        send_dont_have: true,
    }
}

/// Minimalny klient: wysyła wantlistę i czeka na odpowiedź w strumieniu otwartym przez serwer.
struct TestClient {
    control: BitswapControl,
    incoming: BitswapIncomingStreams,
    server: PeerId,
}

impl TestClient {
    async fn connect(address: Multiaddr, server: PeerId) -> Self {
        let mut swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )
            .unwrap()
            .with_behaviour(|_| ipfs_bitswap::new_behaviour())
            .unwrap()
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();
        let mut control = swarm.behaviour().new_control();
        let incoming = control
            .accept(StreamProtocol::new(BITSWAP_PROTOCOL))
            .unwrap();
        swarm.dial(address).unwrap();
        tokio::spawn(async move {
            loop {
                swarm.next().await;
            }
        });
        Self {
            control,
            incoming,
            server,
        }
    }

    async fn ask(&mut self, entries: Vec<WantlistEntry>) -> BitswapMessage {
        let request = BitswapMessage {
            wantlist: Some(Wantlist {
                entries,
                full: false,
            }),
            ..BitswapMessage::default()
        };
        let exchange = async {
            let mut outbound = self
                .control
                .open_stream(self.server, StreamProtocol::new(BITSWAP_PROTOCOL))
                .await
                .expect("open Bitswap stream to the node");
            write_message(&mut outbound, &request, MAX_MESSAGE_BYTES)
                .await
                .unwrap();
            outbound.close().await.unwrap();
            let (peer, mut inbound) = self.incoming.next().await.expect("response stream");
            assert_eq!(peer, self.server);
            read_message(&mut inbound, MAX_MESSAGE_BYTES)
                .await
                .unwrap()
                .expect("response message")
        };
        tokio::time::timeout(Duration::from_secs(WAIT_SECS), exchange)
            .await
            .expect("Bitswap response timed out")
    }
}

fn presence_of(response: &BitswapMessage, cid: &Cid) -> BlockPresenceType {
    let presence = response
        .block_presences
        .iter()
        .find(|presence| presence.cid == cid.to_bytes())
        .expect("presence for CID");
    presence.r#type()
}

#[tokio::test(flavor = "multi_thread")]
async fn node_serves_only_provided_blocks_over_bitswap() {
    let workspace = tempfile::tempdir().unwrap();
    let shared_bytes = b"EHR test block shared over Bitswap\n".to_vec();
    let large_bytes: Vec<u8> = (0..1_500_000u32).map(|index| (index % 251) as u8).collect();
    let private_bytes = b"stored locally, never provided\n".to_vec();
    let mut cids = Vec::new();
    for (name, bytes) in [
        ("shared.txt", &shared_bytes),
        ("large.bin", &large_bytes),
        ("private.txt", &private_bytes),
    ] {
        let path = workspace.path().join(name);
        fs::write(&path, bytes).unwrap();
        cids.push(Cid::decode_from_string(&put_file(workspace.path(), &path)).unwrap());
    }
    let (shared, large, private) = (&cids[0], &cids[1], &cids[2]);

    let port = free_port();
    let mut node = NodeProcess::start(workspace.path(), port, &["node", "start", ""]);
    let server: PeerId = node
        .until("LOCAL_PEER_ID", WAIT_SECS)
        .trim_start_matches("LOCAL_PEER_ID")
        .trim()
        .parse()
        .unwrap();
    node.until("BITSWAP_LISTENING /ipfs/bitswap/1.2.0", WAIT_SECS);
    for cid in [shared, large] {
        node.send(&format!("provide {}", cid.to_string_base32()));
        node.until("DHT_PROVIDE_REQUESTED", WAIT_SECS);
    }

    let address: Multiaddr = format!("/ip4/127.0.0.1/tcp/{port}/p2p/{server}")
        .parse()
        .unwrap();
    let mut client = TestClient::connect(address, server).await;

    // Want-Have: obecność bez przesyłania bajtów.
    let response = client
        .ask(vec![
            want(shared, WantType::Have),
            want(private, WantType::Have),
        ])
        .await;
    assert!(response.payload.is_empty());
    assert_eq!(presence_of(&response, shared), BlockPresenceType::Have);
    assert_eq!(presence_of(&response, private), BlockPresenceType::DontHave);

    // Want-Block: bajty zgodne z CID.
    let response = client.ask(vec![want(shared, WantType::Block)]).await;
    assert_eq!(response.payload.len(), 1);
    assert_eq!(response.payload[0].prefix, [0x01, 0x55, 0x12, 0x20]);
    assert_eq!(response.payload[0].data, shared_bytes);
    assert!(shared.verify_block(&response.payload[0].data));

    let response = client.ask(vec![want(large, WantType::Block)]).await;
    assert_eq!(response.payload.len(), 1);
    assert_eq!(response.payload[0].data, large_bytes);
    assert!(large.verify_block(&response.payload[0].data));

    // Blok zapisany, ale nieogłoszony, nie jest wydawany.
    let response = client.ask(vec![want(private, WantType::Block)]).await;
    assert!(response.payload.is_empty());
    assert_eq!(presence_of(&response, private), BlockPresenceType::DontHave);

    {
        let _held =
            PersistentBlockstore::open_in(&workspace.path().join(DEFAULT_DATA_DIR)).unwrap();
        node.send(&format!("stop {}", shared.to_string_base32()));
        node.until("DHT_STOP_FAILED", WAIT_SECS);
    }
    let response = client.ask(vec![want(shared, WantType::Have)]).await;
    assert_eq!(presence_of(&response, shared), BlockPresenceType::Have);

    // Po `stop` blok przestaje być udostępniany.
    node.send(&format!("stop {}", shared.to_string_base32()));
    node.until("DHT_STOPPED", WAIT_SECS);
    let response = client.ask(vec![want(shared, WantType::Block)]).await;
    assert!(response.payload.is_empty());
    assert_eq!(presence_of(&response, shared), BlockPresenceType::DontHave);
}

fn local_peer_id(node: &NodeProcess) -> String {
    node.until("LOCAL_PEER_ID", WAIT_SECS)
        .trim_start_matches("LOCAL_PEER_ID")
        .trim()
        .to_owned()
}

fn node_dir(workspace: &Path, name: &str) -> std::path::PathBuf {
    let directory = workspace.join(name);
    fs::create_dir(&directory).unwrap();
    directory
}

/// Hub DHT, dostawca i pobierający to trzy osobne węzły: pobierający zna tylko hub,
/// dostawcę znajduje w DHT i sam się z nim łączy.
#[test]
fn node_fetches_verified_block_from_provider_found_in_dht() {
    let workspace = tempfile::tempdir().unwrap();
    let (hub_dir, provider_dir, fetcher_dir) = (
        node_dir(workspace.path(), "hub"),
        node_dir(workspace.path(), "provider"),
        node_dir(workspace.path(), "fetcher"),
    );
    let bytes: Vec<u8> = (0..1_500_000u32).map(|index| (index % 241) as u8).collect();
    let file = workspace.path().join("record.bin");
    fs::write(&file, &bytes).unwrap();
    let cid = put_file(&provider_dir, &file);
    let private_file = workspace.path().join("private.txt");
    fs::write(&private_file, b"stored by the provider, never provided\n").unwrap();
    let private_cid = put_file(&provider_dir, &private_file);

    let hub_port = free_port();
    let hub = NodeProcess::start(&hub_dir, hub_port, &["node", "start", "", "--dht-server"]);
    let hub_address = format!("/ip4/127.0.0.1/tcp/{hub_port}/p2p/{}", local_peer_id(&hub));
    let start = ["node", "start", "--bootstrap", &hub_address];

    let mut provider = NodeProcess::start(&provider_dir, free_port(), &start);
    let provider_id = local_peer_id(&provider);
    provider.until("DHT_BOOTSTRAP_PROGRESS", WAIT_SECS);
    provider.send(&format!("provide {cid}"));
    provider.until("DHT_PROVIDING_STARTED", WAIT_SECS);

    let mut fetcher = NodeProcess::start(&fetcher_dir, free_port(), &start);
    fetcher.until("DHT_BOOTSTRAP_PROGRESS", WAIT_SECS);
    fetcher.send(&format!("fetch {cid}"));
    let received = fetcher.until("BITSWAP_BLOCK_RECEIVED", WAIT_SECS);
    assert!(received.contains(&provider_id), "{received}");
    assert!(received.contains(&cid), "{received}");

    // Blok jest w Blockstore pobierającego i ma te same bajty.
    let output = workspace.path().join("fetched.bin");
    let status = Command::new(cli())
        .current_dir(&fetcher_dir)
        .args(["get", &cid])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);

    fetcher.send(&format!("fetch {cid}"));
    fetcher.until("BITSWAP_ALREADY_LOCAL", WAIT_SECS);

    // Blok nieogłoszony nie jest wydawany, także połączonemu peerowi zapytanemu wprost.
    fetcher.send(&format!("fetch {private_cid}"));
    let failed = fetcher.until("BITSWAP_FETCH_FAILED", 60);
    assert!(failed.contains("does not have the block"), "{failed}");
}

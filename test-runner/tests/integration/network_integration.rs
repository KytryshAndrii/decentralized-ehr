//! Integration: identity persistence, argument validation, connectivity and reconnect.
//! cargo build -p ehr-cli
//! cargo test -p test-runner --test integration-network -- --test-threads=1 --nocapture
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
use test_runner::node_process::{NodeProcess, cli, free_port, put_file};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ehr-network-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn start_args(bootstrap: Option<String>) -> Vec<String> {
    let mut args = vec!["node".into(), "start".into()];
    match bootstrap {
        Some(peer) => {
            args.push("--bootstrap".into());
            args.push(peer);
        }
        None => args.push(String::new()),
    }
    args
}
fn identity(dir: &Path) -> String {
    let output = Command::new(cli())
        .current_dir(dir)
        .args(["node", "identity"])
        .output()
        .expect("failed to run ehr-cli");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("identity output is not UTF-8");
    stdout
        .trim()
        .strip_prefix("LOCAL_PEER_ID")
        .expect("missing LOCAL_PEER_ID prefix")
        .trim()
        .to_owned()
}
#[test]
fn identity_persists_between_processes() {
    let a = Workspace::new();
    let b = Workspace::new();
    assert_eq!(identity(a.path()), identity(a.path()));
    assert_ne!(identity(a.path()), identity(b.path()));
    assert!(a.path().join("tmp/node-key.pb").is_file());
    assert!(b.path().join("tmp/node-key.pb").is_file());
}
#[test]
fn invalid_cli_arguments_fail() {
    let w = Workspace::new();
    // Każdy wariant musi zakończyć się błędem, zanim węzeł wystartuje.
    let peer = "/ip4/127.0.0.1/tcp/4101/p2p/12D3KooWNSJcGB7bK5Kg1gWvyckQ2LbgXB5bMdTUv3Gnji6n8FUz";
    for args in [
        // Adres bez flagi nie jest już obsługiwany: tylko "", --bootstrap, --bootstrap-list.
        vec!["node", "start", peer],
        vec!["node", "start", "--bootstrap"],
        vec!["node", "start", "--bootstrap", "invalid-address"],
        vec!["node", "start", "--bootstrap", "/ip4/127.0.0.1/tcp/4101"],
        vec![
            "node",
            "start",
            "--bootstrap",
            "/ip4/127.0.0.1/tcp/4101/p2p/INVALID",
        ],
        vec!["node", "start", "", "--bootstrap", peer],
        vec!["node", "start", "--bootstrap-list"],
        vec!["node", "start", "--bootstrap-list", "not-a-multiaddr"],
        vec![
            "node",
            "start",
            "--bootstrap-list",
            &format!("{peer},invalid-address"),
        ],
        vec!["node", "start", "", "--dht-server", "--dht-server"],
        vec!["node", "start", "", "--unknown-flag"],
        vec!["node"],
        vec!["node", "unknown"],
        vec!["node", "identity", "old-key.pb"],
        vec!["cid"],
        vec!["cid", "relative/path.txt"],
        vec!["put", "relative/path.txt"],
        vec!["has", "not-a-cid"],
        vec!["get", "not-a-cid", "/tmp/out"],
        vec!["help", "extra"],
        vec!["unknown"],
    ] {
        assert!(
            !Command::new(cli())
                .current_dir(w.path())
                .args(&args)
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success(),
            "{args:?} should fail"
        );
    }
    for args in [vec!["help"], vec!["--help"], vec!["-h"]] {
        assert!(
            Command::new(cli())
                .current_dir(w.path())
                .args(args)
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }
}
/// Plik ponad limit bloku jest odrzucany po rozmiarze, bez wczytywania go do pamięci.
#[test]
fn put_and_cid_reject_oversized_file_before_reading_it() {
    let w = Workspace::new();
    let file = w.path().join("scan.dcm");
    fs::File::create(&file)
        .unwrap()
        .set_len(8 * 1024 * 1024 * 1024)
        .unwrap();
    for command in ["put", "cid"] {
        let output = Command::new(cli())
            .current_dir(w.path())
            .arg(command)
            .arg(&file)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{command} should fail");
        assert!(String::from_utf8_lossy(&output.stderr).contains("BlockTooLarge"));
    }
}
#[test]
fn two_rust_nodes_connect_identify_and_ping() {
    let a = Workspace::new();
    let b = Workspace::new();
    let a_id = identity(a.path());
    let port_a = free_port();
    let port_b = free_port();
    let host_a = NodeProcess::start(a.path(), port_a, &start_args(None));
    host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    let host_b = NodeProcess::start(
        b.path(),
        port_b,
        &start_args(Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}"))),
    );
    host_b.until_all(&["CONNECTED", "IDENTIFY", "PING"], 30);
}
#[test]
fn rust_reconnects_after_peer_restart() {
    let a = Workspace::new();
    let b = Workspace::new();
    let a_id = identity(a.path());
    let port_a = free_port();
    let port_b = free_port();
    let host_a = NodeProcess::start(a.path(), port_a, &start_args(None));
    host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    let host_b = NodeProcess::start(
        b.path(),
        port_b,
        &start_args(Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}"))),
    );
    host_b.until("CONNECTED", 15);
    drop(host_a);
    host_b.until("DISCONNECTED", 15);
    let host_a_again = NodeProcess::start(a.path(), port_a, &start_args(None));
    host_a_again.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    host_b.until("CONNECTED", 45);
    assert_eq!(identity(a.path()), a_id);
}

/// Po restarcie: węzeł bez bootstrapu odnajduje sieć z zapisanych peerów,
/// sam wznawia providing, a klient z listą bootstrap łączy się z oboma serwerami.
#[test]
fn known_peers_and_providing_survive_restart() {
    let a = Workspace::new();
    let b = Workspace::new();
    let c = Workspace::new();
    let (a_id, b_id) = (identity(a.path()), identity(b.path()));
    let (port_a, port_b) = (free_port(), free_port());
    let a_addr = format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}");
    let b_addr = format!("/ip4/127.0.0.1/tcp/{port_b}/p2p/{b_id}");
    let server = |bootstrap: Option<String>| {
        let mut args = start_args(bootstrap);
        args.push("--dht-server".into());
        args
    };

    let file = b.path().join("record.txt");
    std::fs::write(&file, b"synthetic record for reprovide test").unwrap();
    let cid = put_file(b.path(), &file);

    {
        let host_a = NodeProcess::start(a.path(), port_a, &server(None));
        host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
        let mut host_b = NodeProcess::start(b.path(), port_b, &server(Some(a_addr.clone())));
        host_b.until(&format!("KNOWN_PEER_SAVED {a_id}"), 30);
        host_a.until(&format!("KNOWN_PEER_SAVED {b_id}"), 30);
        host_b.send(&format!("provide {cid}"));
        host_b.until("DHT_PROVIDING_STARTED", 30);
    }

    let host_a = NodeProcess::start(a.path(), port_a, &server(None));
    host_a.until(&format!("KNOWN_PEER_RESTORED {b_id}"), 10);
    // B startuje BEZ bootstrapu: zna A wyłącznie z magazynu stanu.
    let host_b = NodeProcess::start(b.path(), port_b, &server(None));
    host_b.until_all(
        &[
            &format!("KNOWN_PEER_RESTORED {a_id}"),
            "DHT_REPROVIDE count=1",
            &format!("CONNECTED {a_id}"),
        ],
        30,
    );
    host_b.until("DHT_PROVIDING_STARTED", 30);

    let mut host_c = NodeProcess::start(
        c.path(),
        free_port(),
        &[
            "node".into(),
            "start".into(),
            "--bootstrap-list".into(),
            format!("{a_addr},{b_addr}"),
        ],
    );
    host_c.until_all(
        &[
            &format!("CONNECTED {a_id}"),
            &format!("CONNECTED {b_id}"),
            "DHT_BOOTSTRAP_STARTED",
        ],
        30,
    );
    host_c.send(&format!("find {cid}"));
    host_c.until(&format!("DHT_PROVIDER {b_id}"), 30);
}

/// Serwer DHT pamięta po restarcie rekord „peer B ma blok X”,
/// nawet gdy B jest już wyłączony i nie może ogłosić go ponownie.
#[test]
fn remote_provider_records_survive_server_restart() {
    let a = Workspace::new();
    let b = Workspace::new();
    let c = Workspace::new();
    let (a_id, b_id) = (identity(a.path()), identity(b.path()));
    let (port_a, port_b) = (free_port(), free_port());
    let a_addr = format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}");
    let server = |bootstrap: Option<String>| {
        let mut args = start_args(bootstrap);
        args.push("--dht-server".into());
        args
    };

    let file = b.path().join("record.txt");
    std::fs::write(&file, b"synthetic record for provider persistence test").unwrap();
    let cid = put_file(b.path(), &file);

    {
        let host_a = NodeProcess::start(a.path(), port_a, &server(None));
        host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
        let mut host_b = NodeProcess::start(b.path(), port_b, &server(Some(a_addr.clone())));
        host_b.until("DHT_BOOTSTRAP_PROGRESS", 30);
        host_b.send(&format!("provide {cid}"));
        host_a.until(&format!("DHT_PROVIDER_RECORD_SAVED {b_id}"), 30);
    }

    // Startuje tylko A; B pozostaje wyłączony.
    let host_a = NodeProcess::start(a.path(), port_a, &server(None));
    host_a.until("DHT_PROVIDER_RECORDS_RESTORED count=1", 10);
    let mut host_c = NodeProcess::start(c.path(), free_port(), &start_args(Some(a_addr)));
    host_c.until("DHT_BOOTSTRAP_STARTED", 30);
    host_c.send(&format!("find {cid}"));
    host_c.until(&format!("DHT_PROVIDER {b_id}"), 30);
}

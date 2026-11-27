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
        let directory = std::env::temp_dir().join(format!(
            "ehr-network-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        Self(directory)
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
fn identity(directory: &Path) -> String {
    let output = Command::new(cli())
        .current_dir(directory)
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
    let workspace_a = Workspace::new();
    let workspace_b = Workspace::new();
    assert_eq!(identity(workspace_a.path()), identity(workspace_a.path()));
    assert_ne!(identity(workspace_a.path()), identity(workspace_b.path()));
    assert!(workspace_a.path().join("tmp/node-key.pb").is_file());
    assert!(workspace_b.path().join("tmp/node-key.pb").is_file());
}
#[test]
fn invalid_cli_arguments_fail() {
    let workspace = Workspace::new();
    // Every variant must fail before the node starts.
    let peer = "/ip4/127.0.0.1/tcp/4101/p2p/12D3KooWNSJcGB7bK5Kg1gWvyckQ2LbgXB5bMdTUv3Gnji6n8FUz";
    for args in [
        // An address without a flag is no longer supported: only "", --bootstrap, --bootstrap-list.
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
                .current_dir(workspace.path())
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
                .current_dir(workspace.path())
                .args(args)
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }
}
/// A file over the block limit is rejected by size, without reading it into memory.
#[test]
fn put_and_cid_reject_oversized_file_before_reading_it() {
    let workspace = Workspace::new();
    let file = workspace.path().join("scan.dcm");
    fs::File::create(&file)
        .unwrap()
        .set_len(8 * 1024 * 1024 * 1024)
        .unwrap();
    for command in ["put", "cid"] {
        let output = Command::new(cli())
            .current_dir(workspace.path())
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
    let workspace_a = Workspace::new();
    let workspace_b = Workspace::new();
    let a_id = identity(workspace_a.path());
    let port_a = free_port();
    let port_b = free_port();
    let host_a = NodeProcess::start(workspace_a.path(), port_a, &start_args(None));
    host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    let host_b = NodeProcess::start(
        workspace_b.path(),
        port_b,
        &start_args(Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}"))),
    );
    host_b.until_all(&["CONNECTED", "IDENTIFY", "PING"], 30);
}
#[test]
fn rust_reconnects_after_peer_restart() {
    let workspace_a = Workspace::new();
    let workspace_b = Workspace::new();
    let a_id = identity(workspace_a.path());
    let port_a = free_port();
    let port_b = free_port();
    let host_a = NodeProcess::start(workspace_a.path(), port_a, &start_args(None));
    host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    let host_b = NodeProcess::start(
        workspace_b.path(),
        port_b,
        &start_args(Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}"))),
    );
    host_b.until("CONNECTED", 15);
    drop(host_a);
    host_b.until("DISCONNECTED", 15);
    let host_a_again = NodeProcess::start(workspace_a.path(), port_a, &start_args(None));
    host_a_again.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    host_b.until("CONNECTED", 45);
    assert_eq!(identity(workspace_a.path()), a_id);
}

/// After a restart: a node without bootstrap finds the network from the saved peers,
/// resumes providing by itself, and a client with a bootstrap list connects to both servers.
#[test]
fn known_peers_and_providing_survive_restart() {
    let workspace_a = Workspace::new();
    let workspace_b = Workspace::new();
    let workspace_c = Workspace::new();
    let (a_id, b_id) = (identity(workspace_a.path()), identity(workspace_b.path()));
    let (port_a, port_b) = (free_port(), free_port());
    let a_addr = format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}");
    let b_addr = format!("/ip4/127.0.0.1/tcp/{port_b}/p2p/{b_id}");
    let server = |bootstrap: Option<String>| {
        let mut args = start_args(bootstrap);
        args.push("--dht-server".into());
        args
    };

    let file = workspace_b.path().join("record.txt");
    std::fs::write(&file, b"synthetic record for reprovide test").unwrap();
    let cid = put_file(workspace_b.path(), &file);

    {
        let host_a = NodeProcess::start(workspace_a.path(), port_a, &server(None));
        host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
        let mut host_b =
            NodeProcess::start(workspace_b.path(), port_b, &server(Some(a_addr.clone())));
        host_b.until(&format!("KNOWN_PEER_SAVED {a_id}"), 30);
        host_a.until(&format!("KNOWN_PEER_SAVED {b_id}"), 30);
        host_b.send(&format!("provide {cid}"));
        host_b.until("DHT_PROVIDING_STARTED", 30);
    }

    let host_a = NodeProcess::start(workspace_a.path(), port_a, &server(None));
    host_a.until(&format!("KNOWN_PEER_RESTORED {b_id}"), 10);
    // B starts WITHOUT bootstrap: it knows A only from the state store.
    let host_b = NodeProcess::start(workspace_b.path(), port_b, &server(None));
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
        workspace_c.path(),
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

/// After a restart a DHT server remembers the record "peer B has block X",
/// even when B is already off and cannot announce it again.
#[test]
fn remote_provider_records_survive_server_restart() {
    let workspace_a = Workspace::new();
    let workspace_b = Workspace::new();
    let workspace_c = Workspace::new();
    let (a_id, b_id) = (identity(workspace_a.path()), identity(workspace_b.path()));
    let (port_a, port_b) = (free_port(), free_port());
    let a_addr = format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}");
    let server = |bootstrap: Option<String>| {
        let mut args = start_args(bootstrap);
        args.push("--dht-server".into());
        args
    };

    let file = workspace_b.path().join("record.txt");
    std::fs::write(&file, b"synthetic record for provider persistence test").unwrap();
    let cid = put_file(workspace_b.path(), &file);

    {
        let host_a = NodeProcess::start(workspace_a.path(), port_a, &server(None));
        host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
        let mut host_b =
            NodeProcess::start(workspace_b.path(), port_b, &server(Some(a_addr.clone())));
        host_b.until("DHT_BOOTSTRAP_PROGRESS", 30);
        host_b.send(&format!("provide {cid}"));
        host_a.until(&format!("DHT_PROVIDER_RECORD_SAVED {b_id}"), 30);
    }

    // Only A starts; B stays off.
    let host_a = NodeProcess::start(workspace_a.path(), port_a, &server(None));
    host_a.until("DHT_PROVIDER_RECORDS_RESTORED count=1", 10);
    let mut host_c = NodeProcess::start(workspace_c.path(), free_port(), &start_args(Some(a_addr)));
    host_c.until("DHT_BOOTSTRAP_STARTED", 30);
    host_c.send(&format!("find {cid}"));
    host_c.until(&format!("DHT_PROVIDER {b_id}"), 30);
}

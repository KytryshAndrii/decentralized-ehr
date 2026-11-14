//! Integration: identity persistence, argument validation, connectivity and reconnect.
//! cargo build -p ehr-cli
//! cargo test -p test-runner --test integration-network -- --test-threads=1 --nocapture
use std::{
    fs,
    io::{BufRead, BufReader},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};
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
fn cli() -> PathBuf {
    if let Some(path) = std::env::var_os("EHR_CLI_BIN") {
        return fs::canonicalize(path).expect("EHR_CLI_BIN must point to a built executable");
    }
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop();
    path.pop();
    path.push(format!("ehr-cli{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "Build the CLI first: cargo build -p ehr-cli"
    );
    path
}
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
struct NodeProcess {
    child: Child,
    lines: Receiver<String>,
    stdin: Option<std::process::ChildStdin>,
}
impl NodeProcess {
    fn start(cwd: &Path, port: u16, args: &[String]) -> Self {
        let mut child = Command::new(cli())
            .current_dir(cwd)
            .env("EHR_LISTEN_PORT", port.to_string())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch ehr-cli");
        let stdin = child.stdin.take();
        let (tx, rx) = mpsc::channel();
        let stdout = child.stdout.take().unwrap();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                println!("[node] {line}");
                let _ = tx.send(line);
            }
        });
        let stderr = child.stderr.take().unwrap();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                eprintln!("[node-err] {line}");
            }
        });
        Self {
            child,
            lines: rx,
            stdin,
        }
    }
    fn send(&mut self, command: &str) {
        use std::io::Write;
        let stdin = self.stdin.as_mut().expect("stdin is piped");
        writeln!(stdin, "{command}").expect("write node command");
        stdin.flush().expect("flush node command");
    }
    fn until_all(&self, patterns: &[&str], secs: u64) {
        let deadline = Instant::now() + Duration::from_secs(secs);
        let mut seen = vec![false; patterns.len()];
        while Instant::now() < deadline {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => {
                    for (index, pattern) in patterns.iter().enumerate() {
                        if line.contains(pattern) {
                            seen[index] = true;
                        }
                    }
                    if seen.iter().all(|v| *v) {
                        return;
                    }
                }
                Err(_) => break,
            }
        }
        panic!("timeout waiting for {patterns:?}; observed {seen:?}");
    }
    fn until(&self, text: &str, secs: u64) -> String {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) if line.contains(text) => return line,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        panic!("timeout waiting for {text:?}");
    }
}
impl Drop for NodeProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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
    for args in [
        vec!["node", "start", "invalid-address"],
        vec!["node", "start", "/ip4/127.0.0.1/tcp/0"],
        vec![
            "node",
            "start",
            "/ip4/127.0.0.1/tcp/4101/p2p/12D3KooWNSJcGB7bK5Kg1gWvyckQ2LbgXB5bMdTUv3Gnji6n8FUz",
        ],
        vec!["node", "identity", "old-key.pb"],
        vec![
            "node",
            "start",
            "",
            "--bootstrap",
            "/ip4/127.0.0.1/tcp/4101/p2p/INVALID",
        ],
        vec!["node", "start", "--bootstrap-list", "not-a-multiaddr"],
        vec!["node", "start", "--bootstrap-list"],
        vec!["unknown"],
    ] {
        assert!(
            !Command::new(cli())
                .current_dir(w.path())
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    for args in [vec!["help"], vec!["--help"], vec!["-h"]] {
        assert!(
            Command::new(cli())
                .current_dir(w.path())
                .args(args)
                .status()
                .unwrap()
                .success()
        );
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
    let output = Command::new(cli())
        .current_dir(b.path())
        .args(["put", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cid = stdout
        .lines()
        .find_map(|line| line.strip_prefix("[BLOCKSTORE] PUT CID="))
        .expect("CID in put output")
        .trim()
        .to_owned();

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
    let output = Command::new(cli())
        .current_dir(b.path())
        .args(["put", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cid = stdout
        .lines()
        .find_map(|line| line.strip_prefix("[BLOCKSTORE] PUT CID="))
        .expect("CID in put output")
        .trim()
        .to_owned();

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

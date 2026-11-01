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
}
impl NodeProcess {
    fn start(cwd: &Path, args: &[String]) -> Self {
        let mut child = Command::new(cli())
            .current_dir(cwd)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch ehr-cli");
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
        Self { child, lines: rx }
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
fn start_args(port: u16, bootstrap: Option<String>) -> Vec<String> {
    let mut args = vec![
        "node".into(),
        "start".into(),
        format!("/ip4/127.0.0.1/tcp/{port}"),
    ];
    if let Some(peer) = bootstrap {
        args.push("--bootstrap".into());
        args.push(peer);
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
        vec!["node", "start"],
        vec!["node", "identity", "old-key.pb"],
        vec![
            "node",
            "start",
            "/ip4/127.0.0.1/tcp/0",
            "--default-bootstrap",
            "--bootstrap",
            "/ip4/127.0.0.1/tcp/4101/p2p/INVALID",
        ],
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
    let host_a = NodeProcess::start(a.path(), &start_args(port_a, None));
    host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    let host_b = NodeProcess::start(
        b.path(),
        &start_args(
            port_b,
            Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}")),
        ),
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
    let host_a = NodeProcess::start(a.path(), &start_args(port_a, None));
    host_a.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    let host_b = NodeProcess::start(
        b.path(),
        &start_args(
            port_b,
            Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}")),
        ),
    );
    host_b.until("CONNECTED", 15);
    drop(host_a);
    host_b.until("DISCONNECTED", 15);
    let host_a_again = NodeProcess::start(a.path(), &start_args(port_a, None));
    host_a_again.until("LISTEN /ip4/127.0.0.1/tcp/", 10);
    host_b.until("CONNECTED", 45);
    assert_eq!(identity(a.path()), a_id);
}

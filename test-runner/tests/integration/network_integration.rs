//! Process-level integration tests. Build first: cargo build -p ehr-cli
//! Then: cargo test -p test-runner --test integration_network -- --test-threads=1 --nocapture
use std::path::{Path, PathBuf};
use std::{
    fs,
    io::{BufRead, BufReader},
    net::TcpListener,
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
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> PathBuf {
    if let Some(path) = std::env::var_os("EHR_CLI_BIN") {
        return path.into();
    }
    // cargo test target/debug/deps/test_binary -> target/debug/ehr-cli
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop();
    path.pop();
    path.push(format!("ehr-cli{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "Build the CLI first: cargo build -p ehr-cli; expected {}",
        path.display()
    );
    path
}
fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}
struct NodeProcess {
    child: Child,
    lines: Receiver<String>,
}
impl NodeProcess {
    fn start(args: &[String]) -> Self {
        let mut child = Command::new(cli())
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
                    for (i, pattern) in patterns.iter().enumerate() {
                        if line.contains(pattern) {
                            seen[i] = true;
                        }
                    }
                    if seen.iter().all(|ok| *ok) {
                        return;
                    }
                }
                Err(_) => break,
            }
        }
        panic!("timed out waiting for {patterns:?}; observed {seen:?}");
    }
    fn until(&self, text: &str, secs: u64) -> String {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(remaining) {
                Ok(line) if line.contains(text) => return line,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        panic!("timed out waiting for output containing {text:?}");
    }
}
impl Drop for NodeProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn start_args(key: &Path, port: u16, bootstrap: Option<String>) -> Vec<String> {
    let mut args = vec![
        "node".into(),
        "start".into(),
        key.display().to_string(),
        format!("/ip4/127.0.0.1/tcp/{port}"),
    ];
    if let Some(b) = bootstrap {
        args.push(b);
    }
    args
}

fn identity(key: &Path) -> String {
    let output = Command::new(cli())
        .args(["node", "identity", &key.display().to_string()])
        .output()
        .expect("failed to run ehr-cli");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("CLI output is not valid UTF-8");

    stdout
        .trim()
        .strip_prefix("LOCAL_PEER_ID")
        .expect("missing LOCAL_PEER_ID prefix")
        .trim()
        .to_string()
}

#[test]
fn identity_persists_between_processes() {
    let w = Workspace::new();
    assert_eq!(
        identity(&w.path("node.key.pb")),
        identity(&w.path("node.key.pb"))
    );
    assert_ne!(identity(&w.path("a.key.pb")), identity(&w.path("b.key.pb")));
}
#[test]
fn invalid_cli_arguments_fail() {
    assert!(
        !Command::new(cli())
            .arg("node")
            .arg("start")
            .status()
            .unwrap()
            .success()
    );
    assert!(
        !Command::new(cli())
            .args(["unknown"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new(cli())
            .arg("--help")
            .status()
            .unwrap()
            .success()
    );
}
#[test]
fn two_rust_nodes_connect_identify_and_ping() {
    let w = Workspace::new();
    let a_key = w.path("a.key.pb");
    let b_key = w.path("b.key.pb");
    let a_id = identity(&a_key);
    let port_a = free_port();
    let port_b = free_port();
    let a = NodeProcess::start(&start_args(&a_key, port_a, None));
    a.until("LISTEN", 10);
    let b = NodeProcess::start(&start_args(
        &b_key,
        port_b,
        Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{a_id}")),
    ));
    b.until_all(&["CONNECTED", "IDENTIFY", "PING"], 30);
}
#[test]
fn rust_reconnects_after_peer_restart() {
    let w = Workspace::new();
    let a_key = w.path("a.key.pb");
    let b_key = w.path("b.key.pb");
    let id_a = identity(&a_key);
    let port_a = free_port();
    let port_b = free_port();
    let a = NodeProcess::start(&start_args(&a_key, port_a, None));
    a.until("LISTEN", 10);
    let b = NodeProcess::start(&start_args(
        &b_key,
        port_b,
        Some(format!("/ip4/127.0.0.1/tcp/{port_a}/p2p/{id_a}")),
    ));
    b.until("CONNECTED", 15);
    drop(a);
    b.until("DISCONNECTED", 15);
    let a2 = NodeProcess::start(&start_args(&a_key, port_a, None));
    a2.until("LISTEN", 10);
    b.until("CONNECTED", 45);
    assert_eq!(identity(&a_key), id_a);
}

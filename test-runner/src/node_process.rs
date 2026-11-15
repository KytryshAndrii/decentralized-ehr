//! Uruchamianie `ehr-cli` jako osobnego procesu w testach integracyjnych.
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

/// Ścieżka do zbudowanego `ehr-cli` (`EHR_CLI_BIN` albo katalog target testu).
pub fn cli() -> PathBuf {
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

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// `ehr-cli put <plik>` w katalogu węzła; zwraca CID jako tekst.
pub fn put_file(cwd: &Path, file: &Path) -> String {
    let output = Command::new(cli())
        .current_dir(cwd)
        .arg("put")
        .arg(file)
        .output()
        .expect("failed to run ehr-cli put");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.split("[BLOCKSTORE] PUT CID=").nth(1))
        .expect("missing PUT CID line")
        .trim()
        .to_owned()
}

pub struct NodeProcess {
    child: Child,
    lines: Receiver<String>,
    stdin: Option<ChildStdin>,
}

impl NodeProcess {
    pub fn start(cwd: &Path, port: u16, args: &[&str]) -> Self {
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

    pub fn send(&mut self, command: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is piped");
        writeln!(stdin, "{command}").expect("write node command");
        stdin.flush().expect("flush node command");
    }

    /// Czeka na linię stdout zawierającą `text` i ją zwraca.
    pub fn until(&self, text: &str, secs: u64) -> String {
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

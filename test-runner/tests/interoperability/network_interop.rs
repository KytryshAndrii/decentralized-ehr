//! Kubo interoperability (not #[ignore]).
//! Requires an installed `ipfs` binary and an initialized dedicated Kubo test repository.
//! When unavailable, prints a clear SKIP (test harness counts it as passed).
use std::{path::PathBuf, process::Command};
#[test]
fn rust_kubo_identify_ping_interoperability() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("test-runner in repo root")
        .to_path_buf();
    let script = workspace.join("scripts/testing/test-network.sh");
    assert!(script.is_file(), "missing script: {}", script.display());
    let ipfs_path = std::env::var_os("IPFS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME missing")).join("ipfs-lab/node-a")
        });
    if Command::new("ipfs").arg("version").output().is_err() {
        eprintln!("SKIP Rust-Kubo: Kubo (`ipfs`) is not installed");
        return;
    }
    if !ipfs_path.join("config").is_file() {
        eprintln!(
            "SKIP Rust-Kubo: dedicated Kubo repo not initialized: {}",
            ipfs_path.display()
        );
        return;
    }
    let cli = std::env::var_os("EHR_CLI_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target/debug/ehr-cli"));
    assert!(
        cli.is_file(),
        "build executable first: cargo build -p ehr-cli"
    );
    let status = Command::new("bash")
        .arg(&script)
        .env("IPFS_PATH", &ipfs_path)
        .env("EHR_CLI_BIN", &cli)
        .current_dir(&workspace)
        .status()
        .expect("cannot launch interoperability script");
    assert!(status.success(), "Rust <-> Kubo test failed: {status}");
}

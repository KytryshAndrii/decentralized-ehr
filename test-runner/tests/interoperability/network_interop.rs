//! Rust <-> Kubo interoperability test.
//! Automatically starts Kubo if necessary.

use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires installed Kubo and built ehr-cli"]
fn rust_kubo_identify_ping_interoperability() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("test-runner must be in repository root")
        .to_path_buf();

    let script = workspace.join("scripts/testing/test-network.sh");

    assert!(script.is_file(), "missing script: {}", script.display());

    // An explicit IPFS_PATH overrides the default test repository.
    let ipfs_path = std::env::var_os("IPFS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = PathBuf::from(
                std::env::var_os("HOME").expect("HOME environment variable is missing"),
            );

            home.join("ipfs-lab/node-a")
        });

    assert!(
        ipfs_path.join("config").is_file(),
        "Kubo repository is not initialized: {}",
        ipfs_path.display()
    );

    let status = Command::new("bash")
        .arg(&script)
        .env("IPFS_PATH", &ipfs_path)
        .current_dir(&workspace)
        .status()
        .expect("failed to launch interoperability script");

    assert!(
        status.success(),
        "Rust <-> Kubo interoperability test failed: {status}"
    );
}

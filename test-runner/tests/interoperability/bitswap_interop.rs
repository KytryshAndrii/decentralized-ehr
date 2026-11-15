use std::{path::PathBuf, process::Command};

#[test]
fn kubo_fetches_provided_block_from_rust_over_bitswap() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("test-runner is inside repository")
        .to_path_buf();
    let script = workspace.join("scripts/testing/test-kubo-bitswap-interop.sh");
    assert!(script.is_file(), "missing script: {}", script.display());
    assert!(
        Command::new("ipfs").arg("version").status().is_ok(),
        "install Kubo and put ipfs in PATH"
    );
    let status = Command::new("bash")
        .arg(&script)
        .current_dir(&workspace)
        .status()
        .expect("failed to launch Kubo interoperability script");
    assert!(
        status.success(),
        "Kubo Bitswap interoperability failed: {status}"
    );
}

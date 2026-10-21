use ipfs_cid::consts::SHA2_256;
use ipfs_cid::models::{Cid, Multihash};
use std::fs;
use std::process::Command;

/// Wywołuje lokalnego klienta Kubo i zwraca wygenerowany CID.
pub fn get_kubo_cid(file_path: &str, codec_str: &str) -> String {
    let output = Command::new("ipfs")
        .env("IPFS_PATH", "~/ipfs-lab/node-a")
        .args([
            "block",
            "put",
            &format!("--cid-codec={}", codec_str),
            "--mhtype=sha2-256",
            "--offline",
            file_path,
        ])
        .output()
        .expect("Nie udało się uruchomić Kubo. Upewnij się, że IPFS jest zainstalowany.");

    assert!(
        output.status.success(),
        "Błąd Kubo: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Generuje CID z pliku przy użyciu Twojego modułu w Rust.
pub fn get_rust_cid(file_path: &str, codec: u64) -> String {
    let data = fs::read(file_path).unwrap_or_else(|_| panic!("Brak pliku: {}", file_path));
    let hash = Multihash::compute(&data, SHA2_256).unwrap();
    let cid = Cid::new_cid(codec, hash);
    cid.to_string_base32()
}

/// Walidator porównujący Rust vs Kubo
pub fn verify_file_interoperability(file_path: &str, rust_codec: u64, kubo_codec_str: &str) {
    let rust_cid = get_rust_cid(file_path, rust_codec);
    let kubo_cid = get_kubo_cid(file_path, kubo_codec_str);

    assert_eq!(
        rust_cid, kubo_cid,
        "Niezgodność CID dla pliku: {}! Rust: {}, Kubo: {}",
        file_path, rust_cid, kubo_cid
    );
}

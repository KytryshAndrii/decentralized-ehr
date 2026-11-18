//! Integration: `ehr-cli add` / `get` on UnixFS files and transfer of a multi-block file
//! between two `ehr-cli` nodes (DHT -> Bitswap -> Blockstore -> rebuilt file).
//! cargo build -p ehr-cli
//! cargo test -p test-runner --test integration-unixfs -- --nocapture
use std::{fs, path::Path, process::Command};
use test_runner::node_process::{NodeProcess, cli, free_port, put_file};

const WAIT_SECS: u64 = 60;

fn patient_text(lines: u32) -> Vec<u8> {
    (0..lines)
        .flat_map(|i| {
            format!(
                "Patient {}: observation {i}, value {} mmHg\n",
                i % 97,
                i * 37 % 200
            )
            .into_bytes()
        })
        .collect()
}

fn run_cli(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new(cli())
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Wartość pola `nazwa=` z linii `[UNIXFS] ADDED ...`.
fn field(line: &str, name: &str) -> String {
    line.split_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {line:?}"))
        .to_owned()
}

fn add(cwd: &Path, file: &Path) -> String {
    let output = run_cli(cwd, &["add", file.to_str().unwrap()]);
    output
        .lines()
        .find(|line| line.starts_with("[UNIXFS] ADDED"))
        .unwrap_or_else(|| panic!("missing ADDED line in {output:?}"))
        .to_owned()
}

fn get(cwd: &Path, cid: &str, output: &Path) -> Vec<u8> {
    run_cli(cwd, &["get", cid, output.to_str().unwrap()]);
    fs::read(output).unwrap()
}

#[test]
fn add_and_get_round_trip_with_chunker_chosen_by_file_type() {
    let workspace = tempfile::tempdir().unwrap();
    let dir = workspace.path();

    // Tekst: Rabin, kilka bloków.
    let text = patient_text(40_000);
    let text_path = dir.join("record.txt");
    fs::write(&text_path, &text).unwrap();
    let added = add(dir, &text_path);
    assert_eq!(field(&added, "chunker"), "rabin-16384-65536-262144");
    assert_eq!(field(&added, "bytes"), text.len().to_string());
    assert!(field(&added, "CID").starts_with("bafybei"), "{added}");
    let first_blocks: usize = field(&added, "new_blocks").parse().unwrap();
    assert!(first_blocks > 3, "{added}");
    assert_eq!(
        get(dir, &field(&added, "CID"), &dir.join("record.out")),
        text
    );

    // Nowa wersja z dopisanym nagłówkiem: większość bloków jest już w Blockstore.
    let mut edited = b"Amended on 2026-11-16\n".to_vec();
    edited.extend_from_slice(&text);
    let edited_path = dir.join("record-v2.txt");
    fs::write(&edited_path, &edited).unwrap();
    let added = add(dir, &edited_path);
    let reused: usize = field(&added, "reused_blocks").parse().unwrap();
    assert!(reused >= first_blocks - 3, "{added}");
    assert_eq!(
        get(dir, &field(&added, "CID"), &dir.join("record-v2.out")),
        edited
    );

    // Wideo: stałe kawałki 256 KiB, ostatni krótszy.
    let video: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
    let video_path = dir.join("ekg.mp4");
    fs::write(&video_path, &video).unwrap();
    let added = add(dir, &video_path);
    assert_eq!(field(&added, "chunker"), "size-262144");
    assert_eq!(field(&added, "new_blocks"), "4");
    assert_eq!(get(dir, &field(&added, "CID"), &dir.join("ekg.out")), video);

    // Mały plik to jeden blok raw: ten sam CID co `put`.
    let note_path = dir.join("note.txt");
    fs::write(&note_path, b"small EHR note\n").unwrap();
    assert_eq!(
        field(&add(dir, &note_path), "CID"),
        put_file(dir, &note_path)
    );

    // `get` nie nadpisuje istniejącego pliku.
    let status = Command::new(cli())
        .current_dir(dir)
        .args(["get", &put_file(dir, &note_path)])
        .arg(dir.join("ekg.out"))
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(fs::read(dir.join("ekg.out")).unwrap(), video);
}

#[test]
fn node_fetches_whole_file_from_another_node() {
    let workspace = tempfile::tempdir().unwrap();
    let (provider_dir, fetcher_dir) = (
        workspace.path().join("provider"),
        workspace.path().join("fetcher"),
    );
    fs::create_dir(&provider_dir).unwrap();
    fs::create_dir(&fetcher_dir).unwrap();

    let text = patient_text(40_000);
    let file = workspace.path().join("record.txt");
    fs::write(&file, &text).unwrap();
    let added = add(&provider_dir, &file);
    let cid = field(&added, "CID");
    let blocks: usize = field(&added, "new_blocks").parse().unwrap();

    let provider_port = free_port();
    let mut provider = NodeProcess::start(
        &provider_dir,
        provider_port,
        &["node", "start", "", "--dht-server"],
    );
    let provider_id = provider
        .until("LOCAL_PEER_ID", WAIT_SECS)
        .trim_start_matches("LOCAL_PEER_ID")
        .trim()
        .to_owned();
    let address = format!("/ip4/127.0.0.1/tcp/{provider_port}/p2p/{provider_id}");
    let mut fetcher = NodeProcess::start(
        &fetcher_dir,
        free_port(),
        &["node", "start", "--bootstrap", &address],
    );
    fetcher.until("DHT_BOOTSTRAP_PROGRESS", WAIT_SECS);

    // Korzeń jest ogłaszany jako ostatni, po wszystkich blokach pliku.
    provider.send(&format!("provide {cid}"));
    provider.until(&format!("DHT_PROVIDE_REQUESTED {cid}"), WAIT_SECS);

    fetcher.send(&format!("fetch {cid}"));
    let done = fetcher.until(&format!("BITSWAP_FETCH_DONE {cid}"), WAIT_SECS);
    assert_eq!(field(&done, "fetched"), blocks.to_string(), "{done}");
    assert_eq!(
        get(&fetcher_dir, &cid, &workspace.path().join("fetched.txt")),
        text
    );

    fetcher.send(&format!("fetch {cid}"));
    fetcher.until("BITSWAP_ALREADY_LOCAL", WAIT_SECS);

    // `stop` wycofuje wszystkie bloki pliku, nie tylko korzeń.
    provider.send(&format!("stop {cid}"));
    for _ in 0..blocks {
        provider.until("DHT_STOPPED", WAIT_SECS);
    }
}

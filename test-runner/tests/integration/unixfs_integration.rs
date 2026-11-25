//! Integration: `ehr-cli add` / `get` on UnixFS files and transfer of a multi-block file
//! between two `ehr-cli` nodes (DHT -> Bitswap -> Blockstore -> rebuilt file).
//! cargo build -p ehr-cli
//! cargo test -p test-runner --test integration-unixfs -- --nocapture
use std::{fs, path::Path, process::Command};
use test_runner::node_process::{NodeProcess, cli, free_port, put_file};

const WAIT_SECS: u64 = 60;

fn patient_text(lines: u32) -> Vec<u8> {
    (0..lines)
        .flat_map(|index| {
            format!(
                "Patient {}: observation {index}, value {} mmHg\n",
                index % 97,
                index * 37 % 200
            )
            .into_bytes()
        })
        .collect()
}

fn run_cli(working_dir: &Path, args: &[&str]) -> String {
    let output = Command::new(cli())
        .current_dir(working_dir)
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

/// Value of the `name=` field of an `[UNIXFS] ADDED ...` line.
fn field(line: &str, name: &str) -> String {
    line.split_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {line:?}"))
        .to_owned()
}

fn add(working_dir: &Path, file: &Path) -> String {
    let output = run_cli(working_dir, &["add", file.to_str().unwrap()]);
    output
        .lines()
        .find(|line| line.starts_with("[UNIXFS] ADDED"))
        .unwrap_or_else(|| panic!("missing ADDED line in {output:?}"))
        .to_owned()
}

fn get(working_dir: &Path, cid: &str, output: &Path) -> Vec<u8> {
    run_cli(working_dir, &["get", cid, output.to_str().unwrap()]);
    fs::read(output).unwrap()
}

#[test]
fn add_and_get_round_trip_with_fixed_chunks() {
    let workspace = tempfile::tempdir().unwrap();
    let directory = workspace.path();

    // Text: fixed 256 KiB chunks, several blocks.
    let text = patient_text(40_000);
    let text_path = directory.join("record.txt");
    fs::write(&text_path, &text).unwrap();
    let added = add(directory, &text_path);
    assert_eq!(field(&added, "chunker"), "size-262144");
    assert_eq!(field(&added, "bytes"), text.len().to_string());
    assert!(field(&added, "CID").starts_with("bafybei"), "{added}");
    let first_blocks: usize = field(&added, "new_blocks").parse().unwrap();
    assert!(first_blocks > 3, "{added}");
    // The same file added twice: every block is already in the Blockstore.
    let again = add(directory, &text_path);
    assert_eq!(field(&again, "new_blocks"), "0", "{again}");
    assert_eq!(
        get(
            directory,
            &field(&added, "CID"),
            &directory.join("record.out")
        ),
        text
    );

    // Video: fixed 256 KiB chunks, the last one shorter.
    let video: Vec<u8> = (0..700_000u32).map(|index| (index % 251) as u8).collect();
    let video_path = directory.join("ekg.mp4");
    fs::write(&video_path, &video).unwrap();
    let added = add(directory, &video_path);
    assert_eq!(field(&added, "chunker"), "size-262144");
    assert_eq!(field(&added, "new_blocks"), "4");
    assert_eq!(
        get(directory, &field(&added, "CID"), &directory.join("ekg.out")),
        video
    );

    // A small file is one raw block: the same CID as `put`.
    let note_path = directory.join("note.txt");
    fs::write(&note_path, b"small EHR note\n").unwrap();
    assert_eq!(
        field(&add(directory, &note_path), "CID"),
        put_file(directory, &note_path)
    );

    // `get` does not overwrite an existing file.
    let status = Command::new(cli())
        .current_dir(directory)
        .args(["get", &put_file(directory, &note_path)])
        .arg(directory.join("ekg.out"))
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(fs::read(directory.join("ekg.out")).unwrap(), video);
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

    // The root is announced last, after every block of the file.
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

    // `stop` withdraws every block of the file, not only the root.
    provider.send(&format!("stop {cid}"));
    for _ in 0..blocks {
        provider.until("DHT_STOPPED", WAIT_SECS);
    }
}

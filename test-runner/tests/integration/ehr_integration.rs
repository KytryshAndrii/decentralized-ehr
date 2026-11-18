//! Integracja EHR na trzech procesach: lekarz (`ehr-cli-doctor`) i dwóch pacjentów
//! (`ehr-cli-patient`), każdy z własnym katalogiem danych, połączeni przez libp2p na localhost.
//! Sprawdza: zaproszenie i wymianę wizytówek, wysłanie pliku (podpis + szyfrowanie + UnixFS),
//! odbiór przez skrzynkę z nazwą pliku zamiast CID, otwarcie po odszyfrowaniu, brak dostępu
//! drugiego pacjenta i zgodność CID szyfrogramu z Kubo.
//! cargo build -p ehr-cli && cargo test -p test-runner --test integration-ehr -- --nocapture
use std::fs;
use std::path::Path;
use std::process::Command;
use test_runner::node_process::{NodeProcess, free_port, role_cli};

const PASSWORD: &str = "synthetic-password-1";
const ENV: &[(&str, &str)] = &[("EHR_PASSWORD", PASSWORD), ("EHR_PUBLIC_IP", "127.0.0.1")];

fn run(binary: &str, cwd: &Path, args: &[&str]) -> String {
    let output = Command::new(role_cli(binary))
        .current_dir(cwd)
        .envs(ENV.iter().copied())
        .args(args)
        .output()
        .expect("run the CLI");
    assert!(
        output.status.success(),
        "{binary} {args:?}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn field(line: &str, name: &str) -> String {
    line.split_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {line:?}"))
        .trim_matches('"')
        .to_owned()
}

fn init(binary: &str, cwd: &Path, name: &str) -> String {
    let output = run(binary, cwd, &["identity", "init", "--name", name]);
    field(
        output.lines().find(|l| l.starts_with("[PROFILE]")).unwrap(),
        "did",
    )
}

fn start(binary: &str, cwd: &Path, port: u16) -> NodeProcess {
    let node = NodeProcess::start_with(&role_cli(binary), cwd, port, &["node", "start", ""], ENV);
    node.until("[PROFILE]", 60);
    node
}

#[test]
fn doctor_sends_an_encrypted_file_that_only_its_patient_opens() {
    let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let (doctor_dir, jan_dir, piotr_dir) = (dirs[0].path(), dirs[1].path(), dirs[2].path());
    init("ehr-cli-doctor", doctor_dir, "dr Anna Malinowska");
    let jan_did = init("ehr-cli-patient", jan_dir, "Jan Kowalski");
    let piotr_did = init("ehr-cli-patient", piotr_dir, "Piotr Wiśniewski");

    let mut doctor = start("ehr-cli-doctor", doctor_dir, free_port());
    doctor.until("BITSWAP_LISTENING", 30);
    let mut jan = start("ehr-cli-patient", jan_dir, free_port());
    let mut piotr = start("ehr-cli-patient", piotr_dir, free_port());

    // Zaproszenie lekarza (jednorazowe) -> pacjent wysyła wizytówkę przez /ehr/contact/1.0.0.
    doctor.send("invite");
    let invite = doctor.until("[INVITE] ehr1:", 30).replace("[INVITE] ", "");
    jan.send(&format!("connect {invite}"));
    jan.until("[CONTACT] DOCTOR_ADDED name=\"dr Anna Malinowska\"", 60);
    doctor.until(&format!("did={jan_did}"), 30);
    piotr.send(&format!("connect {invite}"));
    piotr.until("[EHR_FAILED] rejected: invalid or used invitation", 60);
    doctor.send("invite");
    let second = doctor.until("[INVITE] ehr1:", 30).replace("[INVITE] ", "");
    piotr.send(&format!("connect {second}"));
    piotr.until("[CONTACT] DOCTOR_ADDED", 60);
    doctor.send(&format!("contact accept {jan_did} --name \"Jan Kowalski\""));
    doctor.until("[CONTACT] ACCEPTED Jan Kowalski", 30);
    doctor.send(&format!("contact accept {piotr_did}"));
    doctor.until("[CONTACT] ACCEPTED Piotr", 30);

    // Plik wieloblokowy (> 256 KiB) wystawiony Janowi pod nazwą wyświetlaną.
    let content: Vec<u8> = (0..900_000u32).map(|i| (i % 253) as u8).collect();
    let source = doctor_dir.join("wynik.pdf");
    fs::write(&source, &content).unwrap();
    doctor.send(&format!(
        "doc issue {} --patient Kowalski --name \"Wynik krwi 2026-11-18.pdf\" --category lab",
        source.display()
    ));
    doctor.until(
        "[DOC] ISSUED name=\"Wynik krwi 2026-11-18.pdf\" version=1 seq=1",
        60,
    );

    // Pacjent: skrzynka -> manifest FHIR -> nazwa pliku w rejestrze, bez znajomości CID.
    jan.send("inbox sync");
    jan.until("[INBOX] NEW \"Wynik krwi 2026-11-18.pdf\" verified", 120);
    jan.send("doc list");
    let row = jan.until("[DOC] 1 ", 30);
    assert!(row.contains("\"Wynik krwi 2026-11-18.pdf\""), "{row}");
    assert!(
        row.contains("Wyniki badań") && row.contains("by=\"dr Anna Malinowska\""),
        "{row}"
    );
    assert!(
        !row.contains("bafy"),
        "the list shows names, not CIDs: {row}"
    );
    let opened = jan_dir.join("odebrany.pdf");
    jan.send(&format!("doc open 1 {}", opened.display()));
    jan.until("[DOC] SAVED", 120);
    assert_eq!(fs::read(&opened).unwrap(), content);

    // W IPFS leży szyfrogram: blok nie zawiera treści, a CID jest zgodny z Kubo.
    let manifest = run("ehr-cli-patient", jan_dir, &["doc", "show", "1"]);
    let fhir: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(fhir["resourceType"], "DocumentReference");
    let doc_cid = fhir["content"][0]["attachment"]["url"]
        .as_str()
        .unwrap()
        .trim_start_matches("ipfs://")
        .to_owned();
    jan.send("quit");
    std::thread::sleep(std::time::Duration::from_secs(1));
    let sealed_path = jan_dir.join("sealed.bin");
    run(
        "ehr-cli-patient",
        jan_dir,
        &["get", &doc_cid, sealed_path.to_str().unwrap()],
    );
    let sealed = fs::read(&sealed_path).unwrap();
    assert!(
        !sealed
            .windows(64)
            .any(|window| window == &content[1000..1064])
    );
    if Command::new("ipfs").arg("version").output().is_ok() {
        // Osobne, tymczasowe repozytorium Kubo: `add -n` tylko liczy CID.
        let repo = tempfile::tempdir().unwrap();
        let init = Command::new("ipfs")
            .env("IPFS_PATH", repo.path())
            .args(["init", "--profile=test", "-e"])
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let kubo = Command::new("ipfs")
            .env("IPFS_PATH", repo.path())
            .args([
                "add",
                "-Q",
                "-n",
                "--cid-version=1",
                "--raw-leaves",
                "--chunker=size-262144",
            ])
            .arg(&sealed_path)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&kubo.stdout).trim(), doc_cid);
    } else {
        eprintln!("SKIP Kubo CID check: ipfs is not installed");
    }

    // Drugi pacjent nie dostaje cudzego wpisu; pacjent nie wystawia dokumentów.
    piotr.send("inbox sync");
    piotr.until("[INBOX] SYNCED new=0", 60);
    piotr.send("doc list");
    piotr.until("[DOC] LIST empty", 30);
    piotr.send(&format!(
        "doc issue {} --patient Kowalski",
        source.display()
    ));
    piotr.until("[EHR_FAILED]", 30);
}

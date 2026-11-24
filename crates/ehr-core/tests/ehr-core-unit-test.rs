//! Modele EHR (FHIR R5), rejestr i kontakty, a na końcu pełny przebieg w jednym procesie:
//! lekarz i dwóch pacjentów jako osobne węzły libp2p na localhost.
use ehr_core::utils::{add_years, date_text, iso8601, safe_file_name, size_text};
use ehr_core::{
    Category, ContactKind, DocumentReference, EhrApp, EhrError, IndexEntry, NewDocument, Person,
    Role,
};
use ehr_security::Signed;
use std::path::Path;
use std::sync::Arc;

const PASSWORD: &str = "correct horse battery";
/// 2026-11-18T09:12:00Z
const NOW: u64 = 1_794_993_120;

#[test]
fn utc_calendar_helpers() {
    assert_eq!(iso8601(NOW), "2026-11-18T09:12:00Z");
    assert_eq!(date_text(NOW), "18.11.2026");
    assert_eq!(iso8601(add_years(NOW, 20)), "2046-11-18T09:12:00Z");
    // 29.02.2028 + 20 lat = 29.02.2048 (przestępny); + 1 rok = 28.02.2029.
    let leap_day = 1_835_395_200; // 2028-02-29T00:00:00Z
    assert_eq!(iso8601(leap_day), "2028-02-29T00:00:00Z");
    assert_eq!(iso8601(add_years(leap_day, 20)), "2048-02-29T00:00:00Z");
    assert_eq!(iso8601(add_years(leap_day, 1)), "2029-02-28T00:00:00Z");
    assert_eq!(size_text(41 * 1024), "41 KB");
    assert_eq!(size_text(1_468_006), "1,4 MB");
    assert_eq!(safe_file_name("../../etc/passwd"), "_.._etc_passwd");
    assert_eq!(safe_file_name("..."), "dokument");
}

fn entry() -> IndexEntry {
    IndexEntry {
        seq: 3,
        doc_id: "doc-1".into(),
        version: 2,
        file_name: "Morfologia krwi.pdf".into(),
        media_type: "application/pdf".into(),
        size: 223_232,
        category: Category::Lab,
        manifest_cid: "bafymanifest".into(),
        doc_cid: "bafydocument".into(),
        patient_did: "did:key:z6MkPatient".into(),
        issuer_did: "did:key:z6MkDoctor".into(),
        issuer_name: "dr Anna Malinowska".into(),
        created_utc: NOW,
        valid_until_utc: add_years(NOW, 20),
        prev_manifest_cid: Some("bafyprevious".into()),
        read_token: Signed {
            v: 1,
            signer: "did:key:z6MkDoctor".into(),
            payload: "e30".into(),
            signature: "AA".into(),
        },
    }
}

#[test]
fn manifest_is_a_fhir_r5_document_reference() {
    let entry = entry();
    let manifest = DocumentReference::from_entry(&entry).unwrap();
    let json: serde_json::Value = serde_json::to_value(&manifest).unwrap();
    assert_eq!(json["resourceType"], "DocumentReference");
    assert_eq!(json["status"], "current");
    assert_eq!(json["docStatus"], "final");
    assert_eq!(json["date"], "2026-11-18T09:12:00Z");
    assert_eq!(
        json["subject"]["identifier"]["value"],
        "did:key:z6MkPatient"
    );
    assert_eq!(json["author"][0]["display"], "dr Anna Malinowska");
    assert_eq!(json["category"][0]["coding"][0]["code"], "lab");
    assert_eq!(
        json["relatesTo"][0]["code"]["coding"][0]["code"],
        "replaces"
    );
    let attachment = &json["content"][0]["attachment"];
    assert_eq!(attachment["url"], "ipfs://bafydocument");
    assert_eq!(attachment["title"], "Morfologia krwi.pdf");
    // R5: `size` to integer64, w JSON napis.
    assert_eq!(attachment["size"], "223232");
    // Bez danych osobowych pacjenta w manifeście (tylko DID).
    assert!(!json.to_string().contains("Kowalski"));

    let back = manifest
        .to_entry(3, "bafymanifest", NOW, entry.valid_until_utc)
        .unwrap();
    assert_eq!(back, entry);

    let mut broken = manifest.clone();
    broken.content[0].attachment.url = "https://example.org/x".into();
    assert!(broken.to_entry(3, "m", NOW, 0).is_err());
    let mut broken = manifest;
    broken.resource_type = "Binary".into();
    assert!(broken.to_entry(3, "m", NOW, 0).is_err());
}

#[test]
fn person_is_fhir_patient_or_practitioner() {
    let patient = Person::new(Role::Patient, "did:key:z6Mk1", "Jan Maria Kowalski");
    let json = serde_json::to_value(&patient).unwrap();
    assert_eq!(json["resourceType"], "Patient");
    assert_eq!(json["identifier"][0]["system"], "urn:ietf:rfc:3986");
    assert_eq!(json["name"][0]["family"], "Kowalski");
    assert_eq!(json["name"][0]["given"][0], "Jan Maria");
    assert_eq!(patient.display_name(), "Jan Maria Kowalski");
    assert_eq!(
        serde_json::to_value(Person::new(Role::Doctor, "d", "Malinowska")).unwrap()["resourceType"],
        "Practitioner"
    );
}

#[test]
fn profile_needs_the_password_and_the_matching_role() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!EhrApp::profile_exists(dir.path()));
    assert!(EhrApp::create_profile(dir.path(), Role::Doctor, " ", PASSWORD).is_err());
    let app =
        EhrApp::create_profile(dir.path(), Role::Doctor, "dr Anna Malinowska", PASSWORD).unwrap();
    assert!(EhrApp::profile_exists(dir.path()));
    assert_eq!(
        EhrApp::profile_name(dir.path()).unwrap(),
        "dr Anna Malinowska"
    );
    assert!(EhrApp::create_profile(dir.path(), Role::Doctor, "again", PASSWORD).is_err());
    assert!(matches!(
        EhrApp::open(dir.path(), Role::Patient, PASSWORD),
        Err(EhrError::Role(_))
    ));
    assert!(EhrApp::open(dir.path(), Role::Doctor, "bad password!").is_err());
    let reopened = EhrApp::open(dir.path(), Role::Doctor, PASSWORD).unwrap();
    assert_eq!(reopened.profile(), app.profile());
    // Pacjent-tylko operacje są niedostępne dla lekarza i odwrotnie.
    assert!(matches!(app.find_patient("x"), Err(EhrError::NotFound(_))));
    let patient_dir = tempfile::tempdir().unwrap();
    let patient =
        EhrApp::create_profile(patient_dir.path(), Role::Patient, "Jan Kowalski", PASSWORD)
            .unwrap();
    assert!(matches!(
        patient.create_invite("/ip4/127.0.0.1/tcp/1"),
        Err(EhrError::Role(_))
    ));
    assert!(matches!(patient.find_patient("x"), Err(EhrError::Role(_))));
}

#[test]
fn documents_resolve_by_number_and_unique_name() {
    let mut first = entry();
    first.file_name = "EKG.pdf".into();
    let mut second = entry();
    second.file_name = "EKG.pdf".into();
    second.doc_id = "doc-2".into();
    let mut third = entry();
    third.file_name = "RTG.png".into();
    let list = vec![first.clone(), second, third.clone()];
    assert_eq!(EhrApp::resolve_document(&list, "3").unwrap(), third);
    assert_eq!(EhrApp::resolve_document(&list, "RTG.png").unwrap(), third);
    assert!(matches!(
        EhrApp::resolve_document(&list, "EKG.pdf"),
        Err(EhrError::Ambiguous(candidates)) if candidates == ["#1 EKG.pdf", "#2 EKG.pdf"]
    ));
    assert!(EhrApp::resolve_document(&list, "0").is_err());
    assert!(EhrApp::resolve_document(&list, "4").is_err());
}

fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> T {
    for _ in 0..200 {
        if let Some(value) = check() {
            return value;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("condition not reached");
}

fn listen(port: u16) -> libp2p::Multiaddr {
    format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn app(dir: &Path, role: Role, name: &str) -> Arc<EhrApp> {
    Arc::new(EhrApp::create_profile(dir, role, name, PASSWORD).unwrap())
}

/// Wysłanie pliku, budowa UnixFS, odbiór przez pacjenta i otwarcie po odszyfrowaniu.
#[tokio::test(flavor = "multi_thread")]
async fn doctor_issues_and_only_the_patient_reads_the_document() {
    let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let doctor = app(dirs[0].path(), Role::Doctor, "dr Anna Malinowska");
    let anna = app(dirs[1].path(), Role::Patient, "Jan Kowalski");
    let piotr = app(dirs[2].path(), Role::Patient, "Piotr Wiśniewski");
    let doctor_port = free_port();

    let (doctor_node, _) = doctor
        .clone()
        .start_node(listen(doctor_port), Vec::new(), false)
        .await
        .unwrap();
    let (anna_node, _) = anna
        .clone()
        .start_node(listen(free_port()), Vec::new(), false)
        .await
        .unwrap();
    let (piotr_node, _) = piotr
        .clone()
        .start_node(listen(free_port()), Vec::new(), false)
        .await
        .unwrap();

    // Zaproszenie lekarza -> pacjent wysyła swoją wizytówkę p2p -> lekarz akceptuje.
    let address = format!("/ip4/127.0.0.1/tcp/{doctor_port}");
    let invitation = doctor.create_invite(&address).unwrap();
    let added = anna_node.connect_doctor(&invitation).await.unwrap();
    assert_eq!(added.person.display_name(), "dr Anna Malinowska");
    // Zaproszenie jest jednorazowe.
    assert!(matches!(
        piotr_node.connect_doctor(&invitation).await,
        Err(EhrError::Rejected(_))
    ));
    let piotr_invite = doctor.create_invite(&address).unwrap();
    piotr_node.connect_doctor(&piotr_invite).await.unwrap();
    assert_eq!(doctor.contacts(ContactKind::Pending).unwrap().len(), 2);
    doctor
        .accept_contact(&anna.profile().did, Some("Jan Kowalski"))
        .unwrap();
    doctor.accept_contact(&piotr.profile().did, None).unwrap();
    assert_eq!(
        doctor.find_patient("kowal").unwrap().did,
        anna.profile().did
    );

    // Lekarz wystawia wynik (wieloblokowy: > 256 KiB) Janowi Kowalskiemu.
    let content: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
    let file = dirs[0].path().join("wynik.pdf");
    std::fs::write(&file, &content).unwrap();
    let issued = doctor_node
        .issue_document(NewDocument {
            path: file.clone(),
            patient_did: anna.profile().did.clone(),
            file_name: "Morfologia krwi.pdf".into(),
            media_type: "application/pdf".into(),
            category: Category::Lab,
        })
        .await
        .unwrap();
    assert_eq!((issued.seq, issued.version), (1, 1));
    assert_eq!(issued.valid_until_utc, add_years(issued.created_utc, 20));
    // Lekarz widzi nazwę pliku u pacjenta i otwiera własny dokument.
    let doctor_list = doctor.documents(Some(&anna.profile().did)).unwrap();
    assert_eq!(doctor_list[0].file_name, "Morfologia krwi.pdf");
    assert_eq!(doctor_node.open_document(&issued).await.unwrap(), content);

    // Pacjent: skrzynka -> manifest -> nazwa pliku w rejestrze -> odszyfrowany plik.
    let received = wait_for(|| {
        let runtime = tokio::runtime::Handle::current();
        let synced =
            tokio::task::block_in_place(|| runtime.block_on(anna_node.sync_inbox())).unwrap();
        (!synced.is_empty()).then_some(synced)
    });
    assert_eq!(received.len(), 1);
    let list = anna.documents(None).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].file_name, "Morfologia krwi.pdf");
    assert_eq!(list[0].issuer_name, "dr Anna Malinowska");
    assert_eq!(list[0].category, Category::Lab);
    assert_eq!(anna_node.open_document(&list[0]).await.unwrap(), content);
    let output = dirs[1].path().join("pobrany.pdf");
    anna_node.save_document(&list[0], &output).await.unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), content);
    assert!(
        anna_node.save_document(&list[0], &output).await.is_err(),
        "no overwrite"
    );

    // T05/T06: drugi pacjent nie dostaje wpisu ani nie odszyfruje cudzego dokumentu.
    assert!(piotr_node.sync_inbox().await.unwrap().is_empty());
    assert!(piotr.documents(None).unwrap().is_empty());
    let mut stolen = list[0].clone();
    stolen.patient_did = piotr.profile().did.clone();
    assert!(piotr_node.open_document(&stolen).await.is_err());

    // Pacjent nie wystawia dokumentów (U10).
    assert!(matches!(
        anna_node
            .issue_document(NewDocument {
                path: file.clone(),
                patient_did: piotr.profile().did.clone(),
                file_name: "x".into(),
                media_type: "text/plain".into(),
                category: Category::Other,
            })
            .await,
        Err(EhrError::Role(_))
    ));

    // Zmiana dokumentu: nowa wersja z tym samym doc_id; lista pokazuje ostatnią.
    let corrected = dirs[0].path().join("wynik-v2.pdf");
    std::fs::write(&corrected, b"corrected synthetic result").unwrap();
    let second = doctor_node
        .update_document(&issued, &corrected)
        .await
        .unwrap();
    assert_eq!((second.seq, second.version), (2, 2));
    assert_eq!(
        second.prev_manifest_cid.as_deref(),
        Some(issued.manifest_cid.as_str())
    );
    wait_for(|| {
        let runtime = tokio::runtime::Handle::current();
        let synced =
            tokio::task::block_in_place(|| runtime.block_on(anna_node.sync_inbox())).unwrap();
        (!synced.is_empty()).then_some(())
    });
    let list = anna.documents(None).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].version, 2);
    assert_eq!(
        anna_node.open_document(&list[0]).await.unwrap(),
        b"corrected synthetic result"
    );
}

#[test]
fn doctor_and_patient_started_from_one_folder_keep_separate_profiles() {
    let doctor = EhrApp::default_data_dir(Role::Doctor);
    let patient = EhrApp::default_data_dir(Role::Patient);
    assert_ne!(doctor, patient);
    let root = tempfile::tempdir().unwrap();
    let (doctor, patient) = (root.path().join(doctor), root.path().join(patient));
    for (dir, role) in [(&doctor, Role::Doctor), (&patient, Role::Patient)] {
        std::fs::create_dir_all(dir).unwrap();
        EhrApp::create_profile(dir, role, "Jan Kowalski", PASSWORD).unwrap();
    }
    // Restart: każda rola loguje się hasłem do własnego profilu.
    assert_eq!(
        EhrApp::open(&doctor, Role::Doctor, PASSWORD)
            .unwrap()
            .role(),
        Role::Doctor
    );
    assert_eq!(
        EhrApp::open(&patient, Role::Patient, PASSWORD)
            .unwrap()
            .role(),
        Role::Patient
    );
}

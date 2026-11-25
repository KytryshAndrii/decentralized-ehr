//! Security layer tests: did:key, password key store, OpenPGP (T01–T09 from
//! `docs/phase-1.1-planning-role-models.md`), GnuPG compatibility and UCAN tokens (U01–U11).
use ehr_security::consts::{CMD_DOC_ISSUE, CMD_DOC_READ};
use ehr_security::pgp::{decrypt_and_verify, encrypt_and_sign, fingerprint, parse_public_key};
use ehr_security::{
    CardPayload, ContactCard, Identity, SecurityError, Signed, Token, Verifier, peer_id_from_did,
};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

const PASSWORD: &str = "correct horse battery";
const NOW: u64 = 1_795_000_000;
const TWENTY_YEARS: u64 = 631_152_000;

struct People {
    _dirs: Vec<tempfile::TempDir>,
    doctor: Identity,
    patient: Identity,
    other: Identity,
}

/// Creating keys (Argon2) is expensive, so the identities are shared by the tests.
fn people() -> &'static People {
    static PEOPLE: OnceLock<People> = OnceLock::new();
    PEOPLE.get_or_init(|| {
        let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let create =
            |dir: &tempfile::TempDir| Identity::create(&dir.path().join("keys"), PASSWORD).unwrap();
        People {
            doctor: create(&dirs[0]),
            patient: create(&dirs[1]),
            other: create(&dirs[2]),
            _dirs: dirs,
        }
    })
}

#[test]
fn did_key_round_trips_to_the_peer_id() {
    let doctor = &people().doctor;
    let did = doctor.did();
    assert!(did.starts_with("did:key:z6Mk"), "{did}");
    assert_eq!(peer_id_from_did(&did).unwrap(), doctor.peer_id());
    assert!(peer_id_from_did("did:key:zNotBase58!").is_err());
    assert!(peer_id_from_did("did:web:example.org").is_err());
}

#[test]
fn keystore_needs_the_password_and_never_overwrites() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keys = temp_dir.path().join("keys");
    assert!(matches!(
        Identity::create(&keys, "short"),
        Err(SecurityError::WeakPassword(_))
    ));
    assert!(matches!(
        Identity::unlock(&keys, PASSWORD),
        Err(SecurityError::NoKeys(_))
    ));
    let created = Identity::create(&keys, PASSWORD).unwrap();
    assert!(matches!(
        Identity::create(&keys, PASSWORD),
        Err(SecurityError::KeysExist(_))
    ));
    assert!(matches!(
        Identity::unlock(&keys, "wrong password!"),
        Err(SecurityError::WrongPassword)
    ));
    let unlocked = Identity::unlock(&keys, PASSWORD).unwrap();
    assert_eq!(unlocked.did(), created.did());
    assert_eq!(
        fingerprint(&unlocked.pgp_public),
        fingerprint(&created.pgp_public)
    );
    // The node key is not stored in plain text.
    let sealed = std::fs::read(keys.join("node-key.enc")).unwrap();
    let plain = created.keypair.to_protobuf_encoding().unwrap();
    assert!(
        !sealed
            .windows(plain.len())
            .any(|window| window == plain.as_slice())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&keys), 0o700);
        for file in ["node-key.enc", "pgp-secret.asc", "pgp-public.asc"] {
            assert_eq!(mode(&keys.join(file)), 0o600, "{file}");
        }
    }
}

/// T01, T04, T06, T07, T08, T09.
#[test]
fn document_is_readable_only_by_recipients_and_signed_by_the_doctor() {
    let People {
        doctor,
        patient,
        other,
        ..
    } = people();
    let plain = b"Synthetic lab result: hemoglobin 14.1 g/dL";
    let sealed =
        encrypt_and_sign(plain, doctor, &[&patient.pgp_public, &doctor.pgp_public]).unwrap();
    assert!(!sealed.windows(10).any(|window| window == &plain[..10]));

    assert_eq!(
        decrypt_and_verify(&sealed, patient, &doctor.pgp_public).unwrap(),
        plain
    );
    assert_eq!(
        decrypt_and_verify(&sealed, doctor, &doctor.pgp_public).unwrap(),
        plain
    );
    // T06: another patient has the ciphertext but not the key.
    assert!(matches!(
        decrypt_and_verify(&sealed, other, &doctor.pgp_public),
        Err(SecurityError::DocumentRejected(_))
    ));
    // T08: a file signed by the patient does not pass as the doctor's signature.
    let by_patient = encrypt_and_sign(plain, patient, &[&patient.pgp_public]).unwrap();
    assert!(decrypt_and_verify(&by_patient, patient, &doctor.pgp_public).is_err());
    // T09: changing one ciphertext byte is detected.
    let mut tampered = sealed.clone();
    let middle = tampered.len() / 2;
    tampered[middle] ^= 1;
    assert!(decrypt_and_verify(&tampered, patient, &doctor.pgp_public).is_err());
    // Random session key: the same file gives another ciphertext (so another CID).
    let again = encrypt_and_sign(plain, doctor, &[&patient.pgp_public]).unwrap();
    assert_ne!(again, sealed);
}

fn gpg(home: &Path, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
    use std::io::Write;
    let mut child = Command::new("gpg")
        .env("GNUPGHOME", home)
        .args([
            "--batch",
            "--yes",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            PASSWORD,
        ])
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("gpg");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.unwrap_or_default())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "gpg {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// `gpg --decrypt` decrypts the app ciphertext and the other way round (GnuPG compatibility).
#[test]
fn ciphertext_interoperates_with_gnupg() {
    if Command::new("gpg").arg("--version").output().is_err() {
        eprintln!("SKIP gpg interop: GnuPG is not installed");
        return;
    }
    let People {
        doctor, patient, ..
    } = people();
    let home = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let armored_secret = patient
        .pgp_secret
        .to_armored_string(Default::default())
        .unwrap();
    gpg(home.path(), &["--import"], Some(armored_secret.as_bytes()));
    gpg(
        home.path(),
        &["--import"],
        Some(doctor.pgp_public_armored().unwrap().as_bytes()),
    );

    let plain = b"Synthetic discharge summary";
    let sealed = encrypt_and_sign(plain, doctor, &[&patient.pgp_public]).unwrap();
    assert_eq!(gpg(home.path(), &["--decrypt"], Some(&sealed)), plain);

    let from_gpg = gpg(
        home.path(),
        &[
            "--trust-model",
            "always",
            "--local-user",
            &fingerprint(&patient.pgp_public),
            "--recipient",
            &fingerprint(&patient.pgp_public),
            "--sign",
            "--encrypt",
        ],
        Some(b"written by gpg"),
    );
    assert_eq!(
        decrypt_and_verify(&from_gpg, patient, &patient.pgp_public).unwrap(),
        b"written by gpg"
    );
}

#[test]
fn card_binds_the_pgp_key_and_address_to_the_did() {
    let doctor = &people().doctor;
    let payload = CardPayload {
        did: doctor.did(),
        name: "dr Anna Malinowska".into(),
        role: "doctor".into(),
        pgp_public: doctor.pgp_public_armored().unwrap(),
        multiaddr: Some(format!("/ip4/127.0.0.1/tcp/4101/p2p/{}", doctor.peer_id())),
        invite: Some("one-time-code".into()),
        created_utc: NOW,
    };
    let card = ContactCard::create(doctor, payload.clone()).unwrap();
    let read = ContactCard::from_text(&card.to_text().unwrap()).unwrap();
    assert_eq!(read.payload, payload);

    // A swapped OpenPGP key (another DID than the signature): rejected.
    let mut forged = card.signed.clone();
    let mut changed = payload.clone();
    changed.pgp_public = people().other.pgp_public_armored().unwrap();
    forged.payload = Signed::sign(&people().other.keypair, &changed)
        .unwrap()
        .payload;
    assert!(ContactCard::verify(forged).is_err());

    // An address with someone else's PeerId: rejected.
    let mut wrong_address = payload;
    wrong_address.multiaddr = Some(format!(
        "/ip4/127.0.0.1/tcp/1/p2p/{}",
        people().other.peer_id()
    ));
    assert!(ContactCard::create(doctor, wrong_address).is_err());
    assert!(ContactCard::from_text("ehr1:%%%").is_err());
}

fn trusted() -> Vec<String> {
    vec![people().doctor.did()]
}

fn read_token(issuer: &Identity, root: &Signed, audience: &str, doc: &str) -> Signed {
    Token::delegate_read(issuer, root, audience, doc, NOW, NOW + TWENTY_YEARS).unwrap()
}

fn rejected(result: Result<Token, SecurityError>) -> bool {
    matches!(
        result,
        Err(SecurityError::TokenRejected(_) | SecurityError::BadSignature)
    )
}

#[test]
fn ucan_read_token_is_accepted_only_in_its_scope() {
    let People {
        doctor,
        patient,
        other,
        ..
    } = people();
    let trusted = trusted();
    let verifier = Verifier {
        trusted_issuers: &trusted,
        revoked: &[],
        now: NOW + 10,
    };
    let root = Token::issue_root(doctor, NOW, NOW + TWENTY_YEARS).unwrap();
    assert_eq!(verifier.verify_issue(&root).unwrap().cmd, CMD_DOC_ISSUE);
    let token = read_token(doctor, &root, &patient.did(), "bafydoc");
    let accepted = verifier
        .verify_read(&token, &patient.did(), "bafydoc")
        .unwrap();
    assert_eq!(
        (accepted.cmd.as_str(), accepted.exp),
        (CMD_DOC_READ, NOW + TWENTY_YEARS)
    );

    // U01: after `exp` (20 years) and before `nbf`.
    for now in [NOW + TWENTY_YEARS, NOW - 1] {
        let late = Verifier { now, ..verifier };
        assert!(rejected(late.verify_read(
            &token,
            &patient.did(),
            "bafydoc"
        )));
    }
    // U02: another audience.
    assert!(rejected(verifier.verify_read(
        &token,
        &other.did(),
        "bafydoc"
    )));
    // U03: a `read` token used as `issue`; a document outside `pol`.
    assert!(rejected(verifier.verify_issue(&token)));
    assert!(rejected(verifier.verify_read(
        &token,
        &patient.did(),
        "bafyother"
    )));
    // U06: signature changed by one byte.
    let mut forged = token.clone();
    let mut signature = forged.signature.into_bytes();
    signature[3] = if signature[3] == b'A' { b'B' } else { b'A' };
    forged.signature = String::from_utf8(signature).unwrap();
    assert!(
        verifier
            .verify_read(&forged, &patient.did(), "bafydoc")
            .is_err()
    );
    // U07: revoked token.
    let nonce = token.open::<Token>().unwrap().nonce;
    let revoked = [nonce];
    let with_revocation = Verifier {
        revoked: &revoked,
        ..verifier
    };
    assert!(rejected(with_revocation.verify_read(
        &token,
        &patient.did(),
        "bafydoc"
    )));
}

#[test]
fn ucan_chain_must_start_at_a_trusted_doctor() {
    let People {
        doctor,
        patient,
        other,
        ..
    } = people();
    let trusted = trusted();
    let verifier = Verifier {
        trusted_issuers: &trusted,
        revoked: &[],
        now: NOW + 10,
    };
    let root = Token::issue_root(doctor, NOW, NOW + TWENTY_YEARS).unwrap();

    // U04: broken chain, the token issuer does not own the root.
    let broken = read_token(other, &root, &patient.did(), "bafydoc");
    assert!(rejected(verifier.verify_read(
        &broken,
        &patient.did(),
        "bafydoc"
    )));
    // U05 / U11: a root outside the trusted list.
    let foreign_root = Token::issue_root(other, NOW, NOW + TWENTY_YEARS).unwrap();
    assert!(rejected(verifier.verify_issue(&foreign_root)));
    let foreign = read_token(other, &foreign_root, &patient.did(), "bafydoc");
    assert!(rejected(verifier.verify_read(
        &foreign,
        &patient.did(),
        "bafydoc"
    )));
    // U08 / U09 / U10: a patient without an `issue` chain neither issues nor delegates.
    let patient_root = Token::issue_root(patient, NOW, NOW + TWENTY_YEARS).unwrap();
    let by_patient = read_token(patient, &patient_root, &other.did(), "bafydoc");
    assert!(rejected(verifier.verify_read(
        &by_patient,
        &other.did(),
        "bafydoc"
    )));
    let self_proof = read_token(patient, &root, &other.did(), "bafydoc");
    assert!(rejected(verifier.verify_read(
        &self_proof,
        &other.did(),
        "bafydoc"
    )));
    // An envelope signed by someone other than `iss`.
    let token: Token = read_token(doctor, &root, &patient.did(), "bafydoc")
        .open()
        .unwrap();
    let resigned = Signed::sign(&other.keypair, &token).unwrap();
    assert!(rejected(verifier.verify_read(
        &resigned,
        &patient.did(),
        "bafydoc"
    )));
    assert!(parse_public_key("not a key").is_err());
}

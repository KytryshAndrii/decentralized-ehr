//! Doctor side: issuing and updating documents, answering the inbox and contact requests.
use crate::consts::{
    DOCUMENT_VALIDITY_YEARS, MAX_DOCUMENT_BYTES, MAX_INBOX_ENTRIES, REQUEST_MAX_AGE_SECS,
};
use crate::error::EhrError;
use crate::models::{
    ContactRequest, DocumentReference, EhrApp, EhrNode, InboxItem, InboxRequest, InboxResponse,
    IndexEntry, NewDocument, Role, StreamReply,
};
use crate::node::blocking;
use crate::store::ContactKind;
use crate::utils::{add_years, utc_now_secs};
use crate::wire::read_frame;
use ehr_security::pgp::encrypt_and_sign;
use ehr_security::utils::random_id;
use ehr_security::{ContactCard, Signed, Token, peer_id_from_did};
use ipfs_store::PersistentBlockstore;
use ipfs_unixfs::Chunker;
use libp2p::{PeerId, Stream};
use std::path::{Path, PathBuf};

impl EhrNode {
    /// Issuing: sign + encrypt for {patient, doctor}, UnixFS (fixed chunks), UCAN token
    /// `/ehr/doc/read` for 20 years, encrypted FHIR manifest, pin, provide, index, replica.
    pub async fn issue_document(&self, document: NewDocument) -> Result<IndexEntry, EhrError> {
        self.app.require(Role::Doctor)?;
        let now = utc_now_secs();
        let file_name = crate::utils::file_name_with_extension(&document.file_name, &document.path);
        let entry = IndexEntry {
            seq: 0,
            doc_id: random_id(),
            version: 1,
            media_type: crate::utils::media_type_for(&file_name).into(),
            file_name: crate::utils::safe_file_name(&file_name),
            size: 0,
            category: document.category,
            manifest_cid: String::new(),
            doc_cid: String::new(),
            patient_did: document.patient_did,
            issuer_did: self.app.profile.did.clone(),
            issuer_name: self.app.profile.name.clone(),
            created_utc: now,
            valid_until_utc: add_years(now, DOCUMENT_VALIDITY_YEARS),
            prev_manifest_cid: None,
            // The token is created in `publish`, once the ciphertext CID is known.
            read_token: Signed {
                v: 0,
                signer: String::new(),
                payload: String::new(),
                signature: String::new(),
            },
        };
        self.publish(entry, document.path).await
    }

    /// New version (document change): new ciphertext and manifest with `relatesTo: replaces`.
    pub async fn update_document(
        &self,
        previous: &IndexEntry,
        path: &Path,
    ) -> Result<IndexEntry, EhrError> {
        self.app.require(Role::Doctor)?;
        if previous.issuer_did != self.app.profile.did {
            return Err(EhrError::Rejected(
                "only the issuer can change a document".into(),
            ));
        }
        let now = utc_now_secs();
        let mut entry = previous.clone();
        entry.version += 1;
        entry.file_name = crate::utils::file_name_with_extension(&previous.file_name, path);
        entry.media_type = crate::utils::media_type_for(&entry.file_name).into();
        entry.created_utc = now;
        entry.valid_until_utc = add_years(now, DOCUMENT_VALIDITY_YEARS);
        entry.prev_manifest_cid = Some(previous.manifest_cid.clone());
        self.publish(entry, path.to_path_buf()).await
    }

    async fn publish(&self, entry: IndexEntry, path: PathBuf) -> Result<IndexEntry, EhrError> {
        let (entry, cids) = blocking(&self.app, move |app| {
            let mut entry = entry;
            let patient = app
                .contact(ContactKind::Patient, &entry.patient_did)?
                .ok_or_else(|| EhrError::NotFound(format!("patient {}", entry.patient_did)))?;
            let patient_pgp = ContactCard::from_text(&patient.card)?.pgp;
            if !path.is_absolute() || !path.is_file() {
                return Err(EhrError::Invalid(format!(
                    "not an absolute file path: {}",
                    path.display()
                )));
            }
            entry.size = std::fs::metadata(&path)?.len();
            if entry.size > MAX_DOCUMENT_BYTES {
                return Err(EhrError::Invalid("document too large".into()));
            }
            let identity = &app.identity;
            let recipients = [&patient_pgp, &identity.pgp_public];
            let sealed = encrypt_and_sign(&std::fs::read(&path)?, identity, &recipients)?;
            let store = PersistentBlockstore::open_in(&app.data_dir)?;
            let document = ipfs_unixfs::add_file(&store, sealed.as_slice(), Chunker::fixed())?;
            entry.doc_cid = document.root.to_string_base32();
            // The `issue` root is valid as long as reading, so the chain never expires earlier.
            let root = Token::issue_root(identity, entry.created_utc, entry.valid_until_utc)?;
            entry.read_token = Token::delegate_read(
                identity,
                &root,
                &entry.patient_did,
                &entry.doc_cid,
                entry.created_utc,
                entry.valid_until_utc,
            )?;
            let manifest = serde_json::to_vec(&DocumentReference::from_entry(&entry)?)?;
            let sealed = encrypt_and_sign(&manifest, identity, &recipients)?;
            let manifest = ipfs_unixfs::add_file(&store, sealed.as_slice(), Chunker::fixed())?;
            entry.manifest_cid = manifest.root.to_string_base32();
            let mut cids = ipfs_unixfs::dag_cids(&store, &document.root)?;
            cids.extend(ipfs_unixfs::dag_cids(&store, &manifest.root)?);
            // The issuer is the source of the document, so its copy is never collected.
            for cid in &cids {
                store.pin(cid)?;
            }
            Ok((entry, cids))
        })
        .await?;
        self.provide(cids)?;
        let entry = blocking(&self.app, move |app| app.save_new_entry(entry)).await?;
        println!(
            "[DOC] ISSUED name=\"{}\" version={} seq={} patient={} manifest={}",
            entry.file_name, entry.version, entry.seq, entry.patient_did, entry.manifest_cid
        );
        self.replicate(
            vec![entry.doc_cid.clone(), entry.manifest_cid.clone()],
            peer_id_from_did(&entry.patient_did)?,
        );
        Ok(entry)
    }

    pub(crate) async fn answer_inbox(
        &self,
        peer: PeerId,
        mut stream: Stream,
    ) -> Result<(), EhrError> {
        let signed: Signed = read_frame(&mut stream).await?;
        let request: InboxRequest = signed.open()?;
        // Asked by the DID owner (signature) from its own node (PeerId from the same key).
        if signed.signer != request.patient || peer_id_from_did(&request.patient)? != peer {
            return Err(EhrError::Rejected(
                "request not signed by the patient".into(),
            ));
        }
        if utc_now_secs().abs_diff(request.created_utc) > REQUEST_MAX_AGE_SECS {
            return Err(EhrError::Rejected("stale request".into()));
        }
        let patient = request.patient.clone();
        let since = request.since_seq;
        let entries = blocking(&self.app, move |app| {
            if app.contact(ContactKind::Patient, &patient)?.is_none() {
                return Err(EhrError::Rejected("unknown patient".into()));
            }
            app.entries_since(&patient, &app.profile.did, since)
        })
        .await?;
        let response = InboxResponse {
            patient: request.patient,
            request_nonce: request.nonce,
            entries: entries
                .into_iter()
                .take(MAX_INBOX_ENTRIES)
                .map(|entry| InboxItem {
                    seq: entry.seq,
                    manifest_cid: entry.manifest_cid,
                    created_utc: entry.created_utc,
                })
                .collect(),
        };
        println!(
            "[INBOX] SERVED {} entries={}",
            response.patient,
            response.entries.len()
        );
        self.reply_signed(&mut stream, &response).await
    }

    pub(crate) async fn answer_contact(
        &self,
        peer: PeerId,
        mut stream: Stream,
    ) -> Result<(), EhrError> {
        let signed: Signed = read_frame(&mut stream).await?;
        let request: ContactRequest = signed.open()?;
        let card = ContactCard::from_text(&request.card)?;
        let valid = card.payload.did == signed.signer
            && peer_id_from_did(&card.payload.did)? == peer
            && card.payload.role == "patient";
        let accepted = valid && {
            let code = request.invite.clone();
            blocking(&self.app, move |app| app.take_invite(&code, utc_now_secs())).await?
        };
        if accepted {
            let contact = EhrApp::contact_from_card(&card, Role::Patient)?;
            println!(
                "[CONTACT] REQUEST name=\"{}\" did={}",
                contact.person.display_name(),
                contact.did
            );
            blocking(&self.app, move |app| {
                app.save_contact(ContactKind::Pending, &contact)
            })
            .await?;
        }
        let reply = StreamReply {
            accepted,
            reason: if accepted {
                "pending approval"
            } else {
                "invalid or used invitation"
            }
            .into(),
        };
        self.reply_signed(&mut stream, &reply).await
    }
}

//! Patient side: accepting an invitation, inbox sync, read verification and storage GC.
use crate::consts::{
    BATTERY_CHECK_SECS, CONTACT_PROTOCOL, GC_INTERVAL_SECS, INBOX_POLL_SECS, INBOX_PROTOCOL,
    PATIENT_STORAGE_MAX_BYTES,
};
use crate::error::EhrError;
use crate::models::{
    Contact, ContactRequest, DocumentReference, EhrApp, EhrNode, InboxItem, InboxRequest,
    InboxResponse, IndexEntry, Role, StreamReply,
};
use crate::node::{blocking, network};
use crate::store::ContactKind;
use crate::utils::{patient_dht_server, read_battery, utc_now_secs};
use crate::wire::{read_frame, write_frame};
use ehr_security::pgp::decrypt_and_verify;
use ehr_security::utils::random_id;
use ehr_security::{ContactCard, Signed, Token, Verifier, peer_id_from_did};
use ipfs_network::{BootstrapPeer, DhtMode, NetworkCommand};
use ipfs_store::PersistentBlockstore;
use std::sync::atomic::Ordering;
use std::time::Duration;

impl EhrNode {
    /// The patient pastes a doctor's invitation: the node connects to the doctor and sends its
    /// own card. The doctor is added to the contacts and to the trusted issuers.
    pub async fn connect_doctor(&self, invitation: &str) -> Result<Contact, EhrError> {
        self.app.require(Role::Patient)?;
        let card = ContactCard::from_text(invitation)?;
        let (Some(address), Some(code)) = (&card.payload.multiaddr, &card.payload.invite) else {
            return Err(EhrError::Invalid("not a doctor's invitation".into()));
        };
        if card.payload.role != "doctor" {
            return Err(EhrError::Invalid("not a doctor's invitation".into()));
        }
        let peer = BootstrapPeer::parse(address).map_err(network)?;
        let peer_id = peer.peer_id;
        self.send(NetworkCommand::AddPeer(peer))?;
        let request = ContactRequest {
            invite: code.clone(),
            card: self.app.card(None, None)?.to_text()?,
        };
        let mut stream = self.open(peer_id, CONTACT_PROTOCOL).await?;
        write_frame(
            &mut stream,
            &Signed::sign(&self.app.identity.keypair, &request)?,
        )
        .await?;
        let reply: StreamReply = read_frame::<Signed, _>(&mut stream)
            .await?
            .open_from(&card.payload.did)?;
        if !reply.accepted {
            return Err(EhrError::Rejected(reply.reason));
        }
        let contact = EhrApp::contact_from_card(&card, Role::Doctor)?;
        let saved = contact.clone();
        blocking(&self.app, move |app| {
            app.save_contact(ContactKind::Doctor, &saved)
        })
        .await?;
        println!(
            "[CONTACT] DOCTOR_ADDED name=\"{}\" did={}",
            contact.person.display_name(),
            contact.did
        );
        Ok(contact)
    }

    pub(crate) fn poll_inbox(&self) {
        let node = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(INBOX_POLL_SECS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let _ = node.sync_inbox().await;
            }
        });
    }

    /// Over the limit every unpinned block (the cache) is removed; the patient's own documents are
    /// pinned when downloaded. The first check runs at start.
    pub(crate) fn collect_garbage_periodically(&self) {
        let node = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(GC_INTERVAL_SECS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let removed = blocking(&node.app, |app| {
                    let store = PersistentBlockstore::open_in(&app.data_dir)?;
                    Ok(store.collect_garbage(PATIENT_STORAGE_MAX_BYTES)?)
                })
                .await;
                match removed {
                    Ok(stopped) => {
                        for cid in stopped {
                            let _ = node.send(NetworkCommand::StopProviding(cid));
                        }
                    }
                    Err(error) => eprintln!("[GC_FAILED] {error}"),
                }
            }
        });
    }

    /// A patient started as a DHT server follows the battery (`patient_dht_server`).
    pub(crate) fn watch_battery(&self) {
        let node = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(BATTERY_CHECK_SECS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let battery = read_battery();
                let current = node.dht_server.load(Ordering::Relaxed);
                let server = patient_dht_server(battery, current);
                if server != current {
                    let mode = if server {
                        DhtMode::Server
                    } else {
                        DhtMode::Client
                    };
                    if node.send(NetworkCommand::SetDhtMode(mode)).is_err() {
                        return;
                    }
                    node.dht_server.store(server, Ordering::Relaxed);
                    println!("[BATTERY] {battery:?} DHT {mode:?}");
                }
            }
        });
    }

    /// Asks every doctor from the contacts for new entries and adds them to the patient's index.
    pub async fn sync_inbox(&self) -> Result<Vec<IndexEntry>, EhrError> {
        self.app.require(Role::Patient)?;
        let doctors = blocking(&self.app, |app| app.contacts(ContactKind::Doctor)).await?;
        let mut received = Vec::new();
        for doctor in doctors {
            match self.sync_from(&doctor).await {
                Ok(entries) => received.extend(entries),
                Err(error) => eprintln!("[INBOX_FAILED] {}: {error}", doctor.person.display_name()),
            }
        }
        println!("[INBOX] SYNCED new={}", received.len());
        Ok(received)
    }

    async fn sync_from(&self, doctor: &Contact) -> Result<Vec<IndexEntry>, EhrError> {
        let patient_did = self.app.profile.did.clone();
        let since = {
            let (patient_did, issuer) = (patient_did.clone(), doctor.did.clone());
            blocking(&self.app, move |app| app.last_seq(&patient_did, &issuer)).await?
        };
        let request = InboxRequest {
            patient: patient_did.clone(),
            since_seq: since,
            nonce: random_id(),
            created_utc: utc_now_secs(),
        };
        let mut stream = self
            .open(peer_id_from_did(&doctor.did)?, INBOX_PROTOCOL)
            .await?;
        write_frame(
            &mut stream,
            &Signed::sign(&self.app.identity.keypair, &request)?,
        )
        .await?;
        let response: InboxResponse = read_frame::<Signed, _>(&mut stream)
            .await?
            .open_from(&doctor.did)?;
        if response.patient != patient_did || response.request_nonce != request.nonce {
            return Err(EhrError::Rejected("response for another request".into()));
        }
        let mut items = response.entries;
        items.sort_by_key(|item| item.seq);
        let mut received = Vec::new();
        // Stop at the first error: `since` brings the next attempt back to this entry.
        for item in items.into_iter().filter(|item| item.seq > since) {
            let entry = self.receive(&item, doctor).await?;
            let saved = entry.clone();
            blocking(&self.app, move |app| app.save_entry(&saved)).await?;
            println!("[INBOX] NEW \"{}\" verified", entry.file_name);
            received.push(entry);
        }
        Ok(received)
    }

    /// Manifest: fetch, decrypt, doctor's signature, read token for me, consistency.
    async fn receive(&self, item: &InboxItem, doctor: &Contact) -> Result<IndexEntry, EhrError> {
        let sealed = self.read_file(&item.manifest_cid).await?;
        let doctor_pgp = ContactCard::from_text(&doctor.card)?.pgp;
        let manifest = decrypt_and_verify(&sealed, &self.app.identity, &doctor_pgp)?;
        let document: DocumentReference = serde_json::from_slice(&manifest)?;
        let entry = document.to_entry(item.seq, &item.manifest_cid, item.created_utc, 0)?;
        if entry.patient_did != self.app.profile.did || entry.issuer_did != doctor.did {
            return Err(EhrError::Rejected(
                "manifest for another patient or issuer".into(),
            ));
        }
        let token = self.verify_read(&entry)?;
        Ok(IndexEntry {
            valid_until_utc: token.exp,
            ..entry
        })
    }

    pub(crate) fn verify_read(&self, entry: &IndexEntry) -> Result<Token, EhrError> {
        let trusted = [entry.issuer_did.clone()];
        Ok(Verifier {
            trusted_issuers: &trusted,
            revoked: &[],
            now: utc_now_secs(),
        }
        .verify_read(&entry.read_token, &self.app.profile.did, &entry.doc_cid)?)
    }
}

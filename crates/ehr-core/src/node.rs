//! Operacje sieciowe EHR na działającym węźle IPFS: wystawienie dokumentu (lekarz),
//! skrzynka `/ehr/inbox/1.0.0` i zaproszenia `/ehr/contact/1.0.0`, pobranie i odszyfrowanie.
//! Węzeł jest jedynym właścicielem blockstore w procesie (DEC1); crate'y `ipfs-*` widzą tylko
//! szyfrogramy (encrypt-then-hash).
use crate::consts::{
    CONTACT_PROTOCOL, DOCUMENT_VALIDITY_YEARS, ENV_PUBLIC_IP, INBOX_POLL_SECS, INBOX_PROTOCOL,
    MAX_DOCUMENT_BYTES, MAX_INBOX_ENTRIES, REQUEST_MAX_AGE_SECS, STREAM_TIMEOUT_SECS,
};
use crate::error::EhrError;
use crate::models::{
    Contact, ContactReply, ContactRequest, DocumentReference, EhrApp, EhrNode, InboxItem,
    InboxRequest, InboxResponse, IndexEntry, NewDocument, Role,
};
use crate::store::ContactKind;
use crate::utils::{add_years, utc_now_secs};
use crate::wire::{read_frame, write_frame};
use ehr_security::pgp::{decrypt_and_verify, encrypt_and_sign};
use ehr_security::utils::random_id;
use ehr_security::{ContactCard, Signed, Token, Verifier, peer_id_from_did};
use futures::{AsyncWriteExt, StreamExt};
use ipfs_cid::models::Cid;
use ipfs_network::{
    BootstrapPeer, DhtMode, NetworkCommand, NetworkConfig, NetworkNode, StreamControl,
};
use ipfs_store::PersistentBlockstore;
use ipfs_unixfs::Chunker;
use libp2p::{Multiaddr, PeerId, Stream, StreamProtocol};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Praca blokująca (LevelDB, szyfrowanie, UnixFS) poza wątkami pętli zdarzeń.
async fn blocking<T: Send + 'static>(
    app: &Arc<EhrApp>,
    work: impl FnOnce(&EhrApp) -> Result<T, EhrError> + Send + 'static,
) -> Result<T, EhrError> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || work(&app))
        .await
        .map_err(|error| EhrError::Network(error.to_string()))?
}

fn network(error: impl std::fmt::Display) -> EhrError {
    EhrError::Network(error.to_string())
}

impl EhrApp {
    /// Startuje węzeł IPFS z kluczem odblokowanym hasłem. Lekarz (= placówka) jest serwerem DHT
    /// i obsługuje skrzynkę oraz zaproszenia; pacjent łączy się z lekarzami z kontaktów
    /// i co `INBOX_POLL_SECS` pyta o nowe dokumenty.
    pub async fn start_node(
        self: Arc<Self>,
        listen: Multiaddr,
        mut bootstrap: Vec<BootstrapPeer>,
        dht_server: bool,
    ) -> Result<(EhrNode, JoinHandle<Result<(), String>>), EhrError> {
        if self.role == Role::Patient {
            for doctor in self.contacts(ContactKind::Doctor)? {
                if let Some(peer) = doctor_address(&doctor)?
                    && !bootstrap.iter().any(|known| known.peer_id == peer.peer_id)
                {
                    bootstrap.push(peer);
                }
            }
        }
        let config = NetworkConfig {
            listen,
            state_directory: Some(self.data_dir.clone()),
            keypair: Some(self.identity.keypair.clone()),
            dht_mode: if self.role == Role::Doctor || dht_server {
                DhtMode::Server
            } else {
                DhtMode::Client
            },
            bootstrap,
            ..NetworkConfig::default()
        };
        let node = NetworkNode::new_network_node(config).map_err(network)?;
        let (commands, receiver) = mpsc::unbounded_channel();
        let ehr = EhrNode {
            control: node.stream_control(),
            app: self,
            commands,
        };
        match ehr.app.role {
            Role::Doctor => {
                ehr.serve(INBOX_PROTOCOL)?;
                ehr.serve(CONTACT_PROTOCOL)?;
            }
            Role::Patient => ehr.poll_inbox(),
        }
        let handle = tokio::spawn(async move {
            node.run_with_commands(receiver)
                .await
                .map_err(|error| error.to_string())
        });
        Ok((ehr, handle))
    }

    /// Adres węzła do zaproszenia: `EHR_PUBLIC_IP` albo adres IP interfejsu z trasą
    /// domyślną (bez wysyłania pakietów), port z adresu nasłuchu.
    pub fn node_address(listen: &Multiaddr) -> Result<String, EhrError> {
        let ip = match std::env::var(ENV_PUBLIC_IP) {
            Ok(ip) => ip,
            Err(_) => {
                let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
                socket.connect("192.0.2.1:9")?;
                socket.local_addr()?.ip().to_string()
            }
        };
        let port = listen
            .iter()
            .find_map(|protocol| match protocol {
                libp2p::multiaddr::Protocol::Tcp(port) => Some(port),
                _ => None,
            })
            .ok_or_else(|| EhrError::Invalid("listen address without TCP port".into()))?;
        Ok(format!("/ip4/{ip}/tcp/{port}"))
    }
}

fn doctor_address(doctor: &Contact) -> Result<Option<BootstrapPeer>, EhrError> {
    let card = ContactCard::from_text(&doctor.card)?;
    card.payload
        .multiaddr
        .map(|address| BootstrapPeer::parse(&address).map_err(network))
        .transpose()
}

impl EhrNode {
    pub fn app(&self) -> &Arc<EhrApp> {
        &self.app
    }

    /// Kanał komend węzła IPFS (CLI: `provide`, `fetch`, `peers`, `quit`, ...).
    pub fn commands(&self) -> &mpsc::UnboundedSender<NetworkCommand> {
        &self.commands
    }

    fn send(&self, command: NetworkCommand) -> Result<(), EhrError> {
        self.commands
            .send(command)
            .map_err(|_| EhrError::Network("node stopped".into()))
    }

    fn serve(&self, protocol: StreamProtocol) -> Result<(), EhrError> {
        let mut incoming = self
            .control
            .clone()
            .accept(protocol.clone())
            .map_err(network)?;
        let node = self.clone();
        tokio::spawn(async move {
            while let Some((peer, stream)) = incoming.next().await {
                let node = node.clone();
                let protocol = protocol.clone();
                tokio::spawn(async move {
                    let work = async {
                        if protocol == INBOX_PROTOCOL {
                            node.answer_inbox(peer, stream).await
                        } else {
                            node.answer_contact(peer, stream).await
                        }
                    };
                    let timeout = Duration::from_secs(STREAM_TIMEOUT_SECS);
                    match tokio::time::timeout(timeout, work).await {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => eprintln!("[EHR_FAILED] {protocol} {peer}: {error}"),
                        Err(_) => eprintln!("[EHR_FAILED] {protocol} {peer}: timeout"),
                    }
                });
            }
        });
        Ok(())
    }

    /// Strumień do peera; zaraz po `AddPeer` połączenie może jeszcze powstawać.
    async fn open(&self, peer: PeerId, protocol: StreamProtocol) -> Result<Stream, EhrError> {
        let mut control: StreamControl = self.control.clone();
        let mut last_error = String::new();
        for _ in 0..STREAM_TIMEOUT_SECS * 2 {
            match control.open_stream(peer, protocol.clone()).await {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = error.to_string(),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(EhrError::Network(format!(
            "{peer} unreachable: {last_error}"
        )))
    }

    /// Cały plik lokalnie: z blockstore albo pobrany przez Bitswap od połączonych peerów.
    async fn fetch(&self, cid: &str) -> Result<Cid, EhrError> {
        let cid = Cid::decode_from_string(cid)?;
        let local = {
            let cid = cid.clone();
            blocking(&self.app, move |app| {
                let store = PersistentBlockstore::open_in(&app.data_dir)?;
                Ok(ipfs_unixfs::dag_cids(&store, &cid).is_ok())
            })
            .await?
        };
        if !local {
            let (reply, result) = oneshot::channel();
            self.send(NetworkCommand::FetchWithReply(cid.clone(), reply))?;
            tokio::time::timeout(Duration::from_secs(STREAM_TIMEOUT_SECS * 4), result)
                .await
                .map_err(|_| EhrError::Network("fetch timed out".into()))?
                .map_err(network)?
                .map_err(EhrError::Network)?;
        }
        Ok(cid)
    }

    async fn read_file(&self, cid: &str) -> Result<Vec<u8>, EhrError> {
        let cid = self.fetch(cid).await?;
        blocking(&self.app, move |app| {
            let store = PersistentBlockstore::open_in(&app.data_dir)?;
            let mut bytes = Vec::new();
            ipfs_unixfs::cat(&store, &cid, &mut bytes)?;
            Ok(bytes)
        })
        .await
    }

    // ---------------- lekarz ----------------

    /// Wystawienie: podpis + szyfrowanie dla {pacjent, lekarz}, UnixFS (stałe kawałki),
    /// token UCAN `/ehr/doc/read` na 20 lat, zaszyfrowany manifest FHIR, `provide`, rejestr.
    pub async fn issue_document(&self, document: NewDocument) -> Result<IndexEntry, EhrError> {
        self.app.require(Role::Doctor)?;
        let now = utc_now_secs();
        let entry = IndexEntry {
            seq: 0,
            doc_id: random_id(),
            version: 1,
            file_name: crate::utils::safe_file_name(&document.file_name),
            media_type: document.media_type,
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
            // Token powstaje w `publish`, gdy znany jest CID szyfrogramu.
            read_token: Signed {
                v: 0,
                signer: String::new(),
                payload: String::new(),
                signature: String::new(),
            },
        };
        self.publish(entry, document.path).await
    }

    /// Nowa wersja (zmiana dokumentu): nowy szyfrogram i manifest z `relatesTo: replaces`.
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
        entry.media_type = crate::utils::media_type_for(&path.to_string_lossy()).into();
        entry.created_utc = now;
        entry.valid_until_utc = add_years(now, DOCUMENT_VALIDITY_YEARS);
        entry.prev_manifest_cid = Some(previous.manifest_cid.clone());
        self.publish(entry, path.to_path_buf()).await
    }

    async fn publish(
        &self,
        entry: IndexEntry,
        path: std::path::PathBuf,
    ) -> Result<IndexEntry, EhrError> {
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
            let me = &app.identity;
            let recipients = [&patient_pgp, &me.pgp_public];
            let sealed = encrypt_and_sign(&std::fs::read(&path)?, me, &recipients)?;
            let store = PersistentBlockstore::open_in(&app.data_dir)?;
            let document = ipfs_unixfs::add_file(&store, sealed.as_slice(), Chunker::fixed())?;
            entry.doc_cid = document.root.to_string_base32();
            // Korzeń `issue` ważny tak długo jak odczyt, więc łańcuch nie wygasa wcześniej.
            let root = Token::issue_root(me, entry.created_utc, entry.valid_until_utc)?;
            entry.read_token = Token::delegate_read(
                me,
                &root,
                &entry.patient_did,
                &entry.doc_cid,
                entry.created_utc,
                entry.valid_until_utc,
            )?;
            let manifest = serde_json::to_vec(&DocumentReference::from_entry(&entry)?)?;
            let sealed = encrypt_and_sign(&manifest, me, &recipients)?;
            let manifest = ipfs_unixfs::add_file(&store, sealed.as_slice(), Chunker::fixed())?;
            entry.manifest_cid = manifest.root.to_string_base32();
            let mut cids = ipfs_unixfs::dag_cids(&store, &document.root)?;
            cids.extend(ipfs_unixfs::dag_cids(&store, &manifest.root)?);
            Ok((entry, cids))
        })
        .await?;
        // Liście przed korzeniami: gdy pacjent znajdzie manifest, wszystkie bloki są wydawane.
        for cid in cids.into_iter().rev() {
            self.send(NetworkCommand::StartProviding(cid))?;
        }
        let entry = blocking(&self.app, move |app| app.save_new_entry(entry)).await?;
        println!(
            "[DOC] ISSUED name=\"{}\" version={} seq={} patient={} manifest={}",
            entry.file_name, entry.version, entry.seq, entry.patient_did, entry.manifest_cid
        );
        Ok(entry)
    }

    async fn answer_inbox(&self, peer: PeerId, mut stream: Stream) -> Result<(), EhrError> {
        let signed: Signed = read_frame(&mut stream).await?;
        let request: InboxRequest = signed.open()?;
        // Pyta właściciel DID (podpis) i z własnego węzła (PeerId z tego samego klucza).
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
        write_frame(
            &mut stream,
            &Signed::sign(&self.app.identity.keypair, &response)?,
        )
        .await?;
        Ok(stream.close().await?)
    }

    async fn answer_contact(&self, peer: PeerId, mut stream: Stream) -> Result<(), EhrError> {
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
        let reply = ContactReply {
            accepted,
            reason: if accepted {
                "pending approval"
            } else {
                "invalid or used invitation"
            }
            .into(),
        };
        write_frame(
            &mut stream,
            &Signed::sign(&self.app.identity.keypair, &reply)?,
        )
        .await?;
        Ok(stream.close().await?)
    }

    // ---------------- pacjent ----------------

    /// Pacjent wkleja zaproszenie lekarza: węzeł łączy się z lekarzem i wysyła mu
    /// własną wizytówkę. Lekarz trafia do kontaktów pacjenta i na listę zaufanych wystawców.
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
        let reply: ContactReply = read_frame::<Signed, _>(&mut stream)
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

    fn poll_inbox(&self) {
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

    /// Pyta każdego lekarza z kontaktów o nowe wpisy i dopisuje je do rejestru pacjenta.
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
        let me = self.app.profile.did.clone();
        let since = {
            let (me, issuer) = (me.clone(), doctor.did.clone());
            blocking(&self.app, move |app| app.last_seq(&me, &issuer)).await?
        };
        let request = InboxRequest {
            patient: me.clone(),
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
        if response.patient != me || response.request_nonce != request.nonce {
            return Err(EhrError::Rejected("response for another request".into()));
        }
        let mut items = response.entries;
        items.sort_by_key(|item| item.seq);
        let mut received = Vec::new();
        // Po pierwszym błędzie przerywamy: `since` przy następnej próbie wróci do tego wpisu.
        for item in items.into_iter().filter(|item| item.seq > since) {
            let entry = self.receive(&item, doctor).await?;
            let saved = entry.clone();
            blocking(&self.app, move |app| app.save_entry(&saved)).await?;
            println!("[INBOX] NEW \"{}\" verified", entry.file_name);
            received.push(entry);
        }
        Ok(received)
    }

    /// Manifest: pobranie, odszyfrowanie, podpis lekarza, token odczytu dla mnie, spójność.
    async fn receive(&self, item: &InboxItem, doctor: &Contact) -> Result<IndexEntry, EhrError> {
        let sealed = self.read_file(&item.manifest_cid).await?;
        let doctor_pgp = ContactCard::from_text(&doctor.card)?.pgp;
        let manifest = decrypt_and_verify(&sealed, &self.app.identity, &doctor_pgp)?;
        let document: DocumentReference = serde_json::from_slice(&manifest)?;
        let entry = document.to_entry(item.seq, &item.manifest_cid, item.created_utc, 0)?;
        let me = &self.app.profile.did;
        if &entry.patient_did != me || entry.issuer_did != doctor.did {
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

    fn verify_read(&self, entry: &IndexEntry) -> Result<Token, EhrError> {
        let trusted = [entry.issuer_did.clone()];
        Ok(Verifier {
            trusted_issuers: &trusted,
            revoked: &[],
            now: utc_now_secs(),
        }
        .verify_read(&entry.read_token, &self.app.profile.did, &entry.doc_cid)?)
    }

    // ---------------- obie role ----------------

    /// Odczyt: pacjent wymaga ważnego tokenu (20 lat) od lekarza z kontaktów, lekarz tylko
    /// własnych dokumentów. Zawsze sprawdzany jest podpis OpenPGP wystawcy.
    pub async fn open_document(&self, entry: &IndexEntry) -> Result<Vec<u8>, EhrError> {
        let issuer_pgp = match self.app.role {
            Role::Patient => {
                self.verify_read(entry)?;
                let issuer = entry.issuer_did.clone();
                let doctor = blocking(&self.app, move |app| {
                    app.contact(ContactKind::Doctor, &issuer)?
                        .ok_or_else(|| EhrError::NotFound(format!("doctor {issuer}")))
                })
                .await?;
                ContactCard::from_text(&doctor.card)?.pgp
            }
            Role::Doctor if entry.issuer_did == self.app.profile.did => {
                self.app.identity.pgp_public.clone()
            }
            Role::Doctor => return Err(EhrError::Rejected("document of another issuer".into())),
        };
        let sealed = self.read_file(&entry.doc_cid).await?;
        Ok(decrypt_and_verify(
            &sealed,
            &self.app.identity,
            &issuer_pgp,
        )?)
    }

    /// Zapis odszyfrowanego pliku pod ścieżką podaną przez użytkownika (bez nadpisywania).
    pub async fn save_document(&self, entry: &IndexEntry, output: &Path) -> Result<(), EhrError> {
        if !output.is_absolute() {
            return Err(EhrError::Invalid("output path must be absolute".into()));
        }
        let bytes = self.open_document(entry).await?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        file.write_all(&bytes)?;
        println!(
            "[DOC] SAVED name=\"{}\" path={}",
            entry.file_name,
            output.display()
        );
        Ok(())
    }
}

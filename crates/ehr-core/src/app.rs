//! Operacje lokalne (bez sieci), wspólne dla CLI i UI. Rola pochodzi z binarki;
//! metody spoza roli kończą się błędem `EhrError::Role`.
use crate::consts::{INVITE_VALIDITY_SECS, KEYS_DIR, PROFILE_FILE};
use crate::error::EhrError;
use crate::models::{Contact, EhrApp, IndexEntry, Invite, Person, Profile, Role};
use crate::store::ContactKind;
use crate::utils::utc_now_secs;
use ehr_security::utils::random_id;
use ehr_security::{CardPayload, ContactCard, Identity, peer_id_from_did};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Self::Doctor => "Lekarz",
            Self::Patient => "Pacjent",
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::Patient => "patient",
        }
    }
}

impl EhrApp {
    /// Katalog danych węzła (jak blockstore: `./data` względem katalogu roboczego).
    pub fn default_data_dir() -> PathBuf {
        PathBuf::from(ipfs_store::consts::DEFAULT_DATA_DIR)
    }

    pub fn profile_exists(data_dir: &Path) -> bool {
        data_dir.join(PROFILE_FILE).is_file() && Identity::exists(&data_dir.join(KEYS_DIR))
    }

    /// Pierwsze uruchomienie: klucze chronione hasłem i profil z imieniem i nazwiskiem.
    pub fn create_profile(
        data_dir: &Path,
        role: Role,
        name: &str,
        password: &str,
    ) -> Result<Self, EhrError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(EhrError::Invalid("name must not be empty".into()));
        }
        let identity = Identity::create(&data_dir.join(KEYS_DIR), password)?;
        let profile = Profile {
            name: name.into(),
            role,
            did: identity.did(),
            created_utc: utc_now_secs(),
        };
        fs::write(
            data_dir.join(PROFILE_FILE),
            serde_json::to_vec_pretty(&profile)?,
        )?;
        Ok(Self::assemble(role, data_dir, identity, profile))
    }

    /// Logowanie: hasło odszyfrowuje klucze; profil musi należeć do roli tej binarki.
    pub fn open(data_dir: &Path, role: Role, password: &str) -> Result<Self, EhrError> {
        let profile: Profile = serde_json::from_slice(
            &fs::read(data_dir.join(PROFILE_FILE))
                .map_err(|_| EhrError::NotFound("profile; create it first".into()))?,
        )?;
        if profile.role != role {
            return Err(EhrError::Role(profile.role.code()));
        }
        let identity = Identity::unlock(&data_dir.join(KEYS_DIR), password)?;
        if identity.did() != profile.did {
            return Err(EhrError::Invalid("profile does not match the keys".into()));
        }
        Ok(Self::assemble(role, data_dir, identity, profile))
    }

    /// Nazwa z profilu bez odblokowania kluczy (ekran logowania).
    pub fn profile_name(data_dir: &Path) -> Option<String> {
        let bytes = fs::read(data_dir.join(PROFILE_FILE)).ok()?;
        serde_json::from_slice::<Profile>(&bytes)
            .ok()
            .map(|profile| profile.name)
    }

    fn assemble(role: Role, data_dir: &Path, identity: Identity, profile: Profile) -> Self {
        Self {
            role,
            data_dir: data_dir.to_path_buf(),
            identity,
            profile,
            db_lock: Mutex::new(()),
        }
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub(crate) fn require(&self, role: Role) -> Result<(), EhrError> {
        if self.role == role {
            Ok(())
        } else {
            Err(EhrError::Role(self.role.code()))
        }
    }

    pub(crate) fn card(
        &self,
        multiaddr: Option<String>,
        invite: Option<String>,
    ) -> Result<ContactCard, EhrError> {
        Ok(ContactCard::create(
            &self.identity,
            CardPayload {
                did: self.profile.did.clone(),
                name: self.profile.name.clone(),
                role: self.role.code().into(),
                pgp_public: self.identity.pgp_public_armored()?,
                multiaddr,
                invite,
                created_utc: utc_now_secs(),
            },
        )?)
    }

    /// Lekarz: jednorazowe zaproszenie (wizytówka z adresem węzła i kodem), jak link SimpleX.
    /// `address` to adres bez `/p2p/`, np. `/ip4/192.168.1.125/tcp/4101`.
    pub fn create_invite(&self, address: &str) -> Result<String, EhrError> {
        self.require(Role::Doctor)?;
        let code = random_id();
        let multiaddr = format!("{address}/p2p/{}", self.identity.peer_id());
        let text = self.card(Some(multiaddr), Some(code.clone()))?.to_text()?;
        self.save_invite(&Invite {
            code,
            expires_utc: utc_now_secs() + INVITE_VALIDITY_SECS,
        })?;
        Ok(text)
    }

    /// Lekarz akceptuje prośbę o kontakt; `name` poprawia imię po sprawdzeniu dokumentu.
    pub fn accept_contact(&self, did: &str, name: Option<&str>) -> Result<Contact, EhrError> {
        self.require(Role::Doctor)?;
        let mut contact = self
            .contact(ContactKind::Pending, did)?
            .ok_or_else(|| EhrError::NotFound(format!("contact request {did}")))?;
        if let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) {
            contact.person = Person::new(Role::Patient, did, name);
        }
        self.save_contact(ContactKind::Patient, &contact)?;
        self.remove_contact(ContactKind::Pending, did)?;
        println!("[CONTACT] ACCEPTED {} {did}", contact.person.display_name());
        Ok(contact)
    }

    pub fn reject_contact(&self, did: &str) -> Result<(), EhrError> {
        self.require(Role::Doctor)?;
        self.remove_contact(ContactKind::Pending, did)
    }

    /// Lekarz wybiera pacjenta po imieniu i nazwisku (fragment, bez wielkości liter) albo DID.
    pub fn find_patient(&self, query: &str) -> Result<Contact, EhrError> {
        self.require(Role::Doctor)?;
        let query = query.trim().to_lowercase();
        let patients = self.contacts(ContactKind::Patient)?;
        let mut found: Vec<Contact> = patients
            .into_iter()
            .filter(|contact| {
                contact.did.to_lowercase() == query
                    || contact
                        .person
                        .display_name()
                        .to_lowercase()
                        .contains(&query)
            })
            .collect();
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(EhrError::NotFound(format!("patient {query}"))),
            _ => Err(EhrError::Ambiguous(
                found
                    .iter()
                    .map(|contact| format!("{} ({})", contact.person.display_name(), contact.did))
                    .collect(),
            )),
        }
    }

    /// Wybór dokumentu z listy po numerze (od 1) albo nazwie pliku.
    pub fn resolve_document(
        documents: &[IndexEntry],
        selector: &str,
    ) -> Result<IndexEntry, EhrError> {
        if let Ok(number) = selector.trim().parse::<usize>() {
            return documents
                .get(number.wrapping_sub(1))
                .cloned()
                .ok_or_else(|| EhrError::NotFound(format!("document #{number}")));
        }
        let matches: Vec<&IndexEntry> = documents
            .iter()
            .filter(|entry| entry.file_name == selector.trim())
            .collect();
        match matches.as_slice() {
            [entry] => Ok((*entry).clone()),
            [] => Err(EhrError::NotFound(format!("document {selector}"))),
            many => Err(EhrError::Ambiguous(
                many.iter()
                    .map(|entry| {
                        let number = documents.iter().position(|d| d == *entry).unwrap_or(0) + 1;
                        format!("#{number} {}", entry.file_name)
                    })
                    .collect(),
            )),
        }
    }

    /// Nazwa wystawcy albo pacjenta do wyświetlenia (z kontaktów; DID, gdy nieznany).
    pub fn display_name(&self, did: &str) -> String {
        if did == self.profile.did {
            return self.profile.name.clone();
        }
        let kind = match self.role {
            Role::Doctor => ContactKind::Patient,
            Role::Patient => ContactKind::Doctor,
        };
        self.contact(kind, did)
            .ok()
            .flatten()
            .map(|contact| contact.person.display_name().to_owned())
            .unwrap_or_else(|| did.to_owned())
    }

    pub(crate) fn contact_from_card(card: &ContactCard, role: Role) -> Result<Contact, EhrError> {
        peer_id_from_did(&card.payload.did)?;
        Ok(Contact {
            did: card.payload.did.clone(),
            card: card.to_text()?,
            person: Person::new(role, &card.payload.did, &card.payload.name),
            added_utc: utc_now_secs(),
        })
    }
}

use ehr_security::{Identity, Signed};
use ipfs_network::{NetworkCommand, StreamControl};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

/// Rola węzła. Ustalana przy kompilacji (osobne binarki lekarza i pacjenta);
/// lekarz i placówka to jeden węzeł (założenie Z1 planu phase-9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Doctor,
    Patient,
}

/// Kategoria dokumentu (`DocumentReference.category`), widoczna w panelu pacjenta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Lab,
    Imaging,
    Discharge,
    Other,
}

// ---------- HL7 FHIR R5: minimalne podzbiory zasobów (nazwy pól jak w specyfikacji) ----------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identifier {
    pub system: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanName {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub given: Vec<String>,
}

/// `Patient` (u lekarza) albo `Practitioner` (u pacjenta): DID jako identyfikator i nazwa.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub resource_type: String,
    pub identifier: Vec<Identifier>,
    pub active: bool,
    pub name: Vec<HumanName>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coding {
    pub system: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeableConcept {
    pub coding: Vec<Coding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<Identifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeableReference {
    pub reference: Reference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatesTo {
    pub code: CodeableConcept,
    pub target: CodeableReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub content_type: String,
    /// `ipfs://<CID szyfrogramu>`.
    pub url: String,
    /// `integer64` w FHIR R5 jest w JSON napisem.
    pub size: String,
    pub title: String,
    pub creation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    pub attachment: Attachment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_string: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_date_time: Option<String>,
}

/// Manifest dokumentu = zasób FHIR R5 `DocumentReference`. W IPFS leży tylko zaszyfrowany
/// (OpenPGP dla pacjenta i lekarza), więc nazwa pliku i strony nie są jawne.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentReference {
    pub resource_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extension: Vec<Extension>,
    pub identifier: Vec<Identifier>,
    pub version: String,
    pub status: String,
    pub doc_status: String,
    pub category: Vec<CodeableConcept>,
    pub subject: Reference,
    pub date: String,
    pub author: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relates_to: Vec<RelatesTo>,
    pub content: Vec<Content>,
}

// ---------- Dane lokalne ----------

/// Profil węzła w `data/profile.json` (jawny; klucze leżą osobno, zaszyfrowane hasłem).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub role: Role,
    pub did: String,
    pub created_utc: u64,
}

/// Kontakt w `contacts.leveldb`: wizytówka (DID, klucz OpenPGP, adres) i zasób FHIR osoby.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub did: String,
    pub card: String,
    pub person: Person,
    pub added_utc: u64,
}

/// Wydane, jeszcze niewykorzystane zaproszenie lekarza.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub code: String,
    pub expires_utc: u64,
}

/// Wpis rejestru dokumentów: nazwa pliku zamiast CID w interfejsie.
/// Klucz: `/index/<DID pacjenta>/<DID wystawcy>/<seq big-endian>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub seq: u64,
    pub doc_id: String,
    pub version: u32,
    pub file_name: String,
    pub media_type: String,
    pub size: u64,
    pub category: Category,
    pub manifest_cid: String,
    pub doc_cid: String,
    pub patient_did: String,
    pub issuer_did: String,
    pub issuer_name: String,
    pub created_utc: u64,
    pub valid_until_utc: u64,
    pub prev_manifest_cid: Option<String>,
    pub read_token: Signed,
}

/// Dokument do wystawienia przez lekarza.
#[derive(Clone, Debug)]
pub struct NewDocument {
    pub path: PathBuf,
    pub patient_did: String,
    pub file_name: String,
    pub media_type: String,
    pub category: Category,
}

// ---------- Wiadomości protokołów (JSON w kopercie `Signed`) ----------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxRequest {
    pub patient: String,
    pub since_seq: u64,
    pub nonce: String,
    pub created_utc: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    pub seq: u64,
    pub manifest_cid: String,
    pub created_utc: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxResponse {
    pub patient: String,
    pub request_nonce: String,
    pub entries: Vec<InboxItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactRequest {
    pub invite: String,
    /// Tekst wizytówki pacjenta (`ehr1:...`).
    pub card: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactReply {
    pub accepted: bool,
    pub reason: String,
}

/// Wspólna logika aplikacji lekarza i pacjenta (CLI i UI korzystają z tych samych metod).
pub struct EhrApp {
    pub(crate) role: Role,
    pub(crate) data_dir: PathBuf,
    pub(crate) identity: Identity,
    pub(crate) profile: Profile,
    /// Kontakty i rejestr otwierane na czas operacji; zamek szereguje operacje w procesie.
    pub(crate) db_lock: Mutex<()>,
}

/// Uruchomiony węzeł EHR: komendy do węzła IPFS i uchwyt strumieni protokołów EHR.
#[derive(Clone)]
pub struct EhrNode {
    pub(crate) app: Arc<EhrApp>,
    pub(crate) commands: UnboundedSender<NetworkCommand>,
    pub(crate) control: StreamControl,
}

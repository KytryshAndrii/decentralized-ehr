use ehr_security::{Identity, Signed};
use ipfs_network::{NetworkCommand, StreamControl};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

/// Node role, fixed at compile time (separate doctor and patient binaries);
/// the doctor and the facility are one node (assumption Z1 of the phase-9 plan).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Doctor,
    Patient,
}

/// Document category (`DocumentReference.category`), shown in the patient's panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Lab,
    Imaging,
    Discharge,
    Other,
}

// ---------- HL7 FHIR R5: minimal resource subsets (field names as in the specification) ----------

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

/// `Patient` (on the doctor side) or `Practitioner` (on the patient side): DID as identifier, and a name.
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
    /// `integer64` in FHIR R5 is a string in JSON.
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

/// Document manifest = FHIR R5 `DocumentReference` resource. IPFS holds it only encrypted
/// (OpenPGP for the patient and the doctor), so the file name and the parties are not public.
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

/// Node profile in `<role folder>/profile.json` (plain; the keys are separate, password-encrypted).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub role: Role,
    pub did: String,
    pub created_utc: u64,
}

/// Contact in `contacts.leveldb`: contact card (DID, OpenPGP key, address) and the person's FHIR resource.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub did: String,
    pub card: String,
    pub person: Person,
    pub added_utc: u64,
}

/// An issued, not yet used doctor's invitation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub code: String,
    pub expires_utc: u64,
}

/// Document index entry: the UI shows the file name instead of the CID.
/// Key: `/index/<patient DID>/<issuer DID>/<seq big-endian>`.
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

#[derive(Clone, Debug)]
pub struct NewDocument {
    pub path: PathBuf,
    pub patient_did: String,
    /// Display name; the extension and the MIME type come from `path` (see `file_name_with_extension`).
    pub file_name: String,
    pub category: Category,
}

// ---------- Protocol messages (JSON inside a `Signed` envelope) ----------

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
    /// Text of the patient's contact card (`ehr1:...`).
    pub card: String,
}

/// Answer to a contact or replication request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamReply {
    pub accepted: bool,
    pub reason: String,
}

/// The doctor asks a DHT server to keep a copy of a document (ciphertext and manifest).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicateRequest {
    pub cids: Vec<String>,
    pub created_utc: u64,
}

/// Shared logic of the doctor and patient apps (the CLI and the UI call the same methods).
pub struct EhrApp {
    pub(crate) role: Role,
    pub(crate) data_dir: PathBuf,
    pub(crate) identity: Identity,
    pub(crate) profile: Profile,
    /// Contacts and index are opened per operation; the lock serializes operations in the process.
    pub(crate) db_lock: Mutex<()>,
}

/// Running EHR node: commands to the IPFS node and the stream handle of the EHR protocols.
#[derive(Clone)]
pub struct EhrNode {
    pub(crate) app: Arc<EhrApp>,
    pub(crate) commands: UnboundedSender<NetworkCommand>,
    pub(crate) control: StreamControl,
    /// Changes at runtime for a patient on battery (see `watch_battery`).
    pub(crate) dht_server: Arc<std::sync::atomic::AtomicBool>,
}

/// Battery state read from the operating system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

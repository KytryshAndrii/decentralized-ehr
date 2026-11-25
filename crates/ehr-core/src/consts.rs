use libp2p::StreamProtocol;

/// Inbox: the patient asks the doctor for new documents (`docs/phase-9-ehr-delivery-plan.md` §4.5).
pub const INBOX_PROTOCOL: StreamProtocol = StreamProtocol::new("/ehr/inbox/1.0.0");
/// Answer to an invitation: the patient sends its own card to the doctor.
pub const CONTACT_PROTOCOL: StreamProtocol = StreamProtocol::new("/ehr/contact/1.0.0");
/// Replication: the doctor asks another DHT server to fetch, pin and provide a new document.
pub const REPLICATE_PROTOCOL: StreamProtocol = StreamProtocol::new("/ehr/replicate/1.0.0");

/// Role folders created in the working directory; the CLI and the desktop app of one role share them.
pub const DOCTOR_DATA_DIR: &str = "ehr-doctor-data";
pub const PATIENT_DATA_DIR: &str = "ehr-patient-data";

/// Folders and files inside the node data folder (next to `ehr-blockstore.leveldb`).
pub const KEYS_DIR: &str = "keys";
pub const PROFILE_FILE: &str = "profile.json";
pub const CONTACTS_DB_DIR: &str = "contacts.leveldb";
pub const INDEX_DB_DIR: &str = "index.leveldb";

/// Prefixes in `contacts.leveldb` and `index.leveldb`.
pub const PATIENT_PREFIX: &str = "/patient/";
pub const DOCTOR_PREFIX: &str = "/doctor/";
pub const PENDING_PREFIX: &str = "/pending/";
pub const INVITE_PREFIX: &str = "/invite/";
pub const INDEX_PREFIX: &str = "/index/";

/// Medical records in Poland: 20 years; the read token is valid that long from the issue date.
pub const DOCUMENT_VALIDITY_YEARS: i64 = 20;
/// A doctor's invitation (single use) expires after 1 hour.
pub const INVITE_VALIDITY_SECS: u64 = 3600;
/// An inbox request older than this window is rejected (replay protection).
pub const REQUEST_MAX_AGE_SECS: u64 = 300;
/// How often the patient node asks the doctors for new documents, in seconds.
pub const INBOX_POLL_SECS: u64 = 30;
pub const MAX_WIRE_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_INBOX_ENTRIES: usize = 500;
/// shortcut: encryption happens in memory, so a document is at most 256 MiB; larger files need streaming OpenPGP.
pub const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
pub const STREAM_TIMEOUT_SECS: u64 = 30;
/// A replica downloads the document and the manifest before it answers.
pub const REPLICATE_TIMEOUT_SECS: u64 = 300;
/// A replication request carries only the document and its manifest.
pub const REPLICATE_MAX_CIDS: usize = 2;
pub const ENV_PUBLIC_IP: &str = "EHR_PUBLIC_IP";

/// Patient storage limit; above it unpinned blocks are removed (Kubo `Datastore.StorageMax`, default 10GB).
pub const PATIENT_STORAGE_MAX_BYTES: u64 = 10_000_000_000;
/// How often the patient checks the limit (Kubo `Datastore.GCPeriod`, default 1h).
pub const GC_INTERVAL_SECS: u64 = 3600;

/// HL7 FHIR R5 (5.0.0).
pub const FHIR_VERSION: &str = "5.0.0";
pub const FHIR_DID_SYSTEM: &str = "urn:ietf:rfc:3986";
pub const FHIR_DOC_ID_SYSTEM: &str = "urn:ehr:document-id";
pub const FHIR_CATEGORY_SYSTEM: &str = "urn:ehr:document-category";
pub const FHIR_RELATIONSHIP_SYSTEM: &str = "http://hl7.org/fhir/document-relationship-type";
pub const FHIR_READ_TOKEN_EXTENSION: &str = "urn:ehr:ucan-read-token";
pub const FHIR_VALID_UNTIL_EXTENSION: &str = "urn:ehr:valid-until";
pub const IPFS_URL_PREFIX: &str = "ipfs://";

/// Patient in DHT server mode: below this battery level it switches to DHT client.
pub const BATTERY_CLIENT_BELOW_PERCENT: u8 = 33;
/// Back to DHT server from this level (or while charging); the gap stops the mode flapping.
pub const BATTERY_SERVER_FROM_PERCENT: u8 = 40;
/// How often the patient reads the battery level.
pub const BATTERY_CHECK_SECS: u64 = 60;
/// Linux (also WSL2) exposes batteries here; `capacity` is in percent.
pub const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply";

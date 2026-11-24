use libp2p::StreamProtocol;

/// Skrzynka: pacjent pyta lekarza o nowe dokumenty (`docs/phase-9-ehr-delivery-plan.md` §4.5).
pub const INBOX_PROTOCOL: StreamProtocol = StreamProtocol::new("/ehr/inbox/1.0.0");
/// Odpowiedź na zaproszenie: pacjent wysyła lekarzowi własną wizytówkę.
pub const CONTACT_PROTOCOL: StreamProtocol = StreamProtocol::new("/ehr/contact/1.0.0");

/// Katalogi i pliki w katalogu danych węzła (obok `ehr-blockstore.leveldb`).
pub const KEYS_DIR: &str = "keys";
pub const PROFILE_FILE: &str = "profile.json";
pub const CONTACTS_DB_DIR: &str = "contacts.leveldb";
pub const INDEX_DB_DIR: &str = "index.leveldb";

/// Prefiksy w `contacts.leveldb` i `index.leveldb`.
pub const PATIENT_PREFIX: &str = "/patient/";
pub const DOCTOR_PREFIX: &str = "/doctor/";
pub const PENDING_PREFIX: &str = "/pending/";
pub const INVITE_PREFIX: &str = "/invite/";
pub const INDEX_PREFIX: &str = "/index/";

/// Dokumentacja medyczna w Polsce: 20 lat; tyle ważny jest token odczytu od daty wystawienia.
pub const DOCUMENT_VALIDITY_YEARS: i64 = 20;
/// Zaproszenie lekarza (jednorazowe) wygasa po 1 godzinie.
pub const INVITE_VALIDITY_SECS: u64 = 3600;
/// Żądanie skrzynki starsze niż to okno jest odrzucane (ochrona przed powtórzeniem).
pub const REQUEST_MAX_AGE_SECS: u64 = 300;
/// Co tyle sekund węzeł pacjenta pyta lekarzy o nowe dokumenty.
pub const INBOX_POLL_SECS: u64 = 30;
pub const MAX_WIRE_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_INBOX_ENTRIES: usize = 500;
/// shortcut: szyfrowanie w pamięci, więc dokument do 256 MiB; większe pliki wymagają strumieniowego OpenPGP.
pub const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
pub const STREAM_TIMEOUT_SECS: u64 = 30;
pub const ENV_PUBLIC_IP: &str = "EHR_PUBLIC_IP";

/// HL7 FHIR R5 (5.0.0).
pub const FHIR_VERSION: &str = "5.0.0";
pub const FHIR_DID_SYSTEM: &str = "urn:ietf:rfc:3986";
pub const FHIR_DOC_ID_SYSTEM: &str = "urn:ehr:document-id";
pub const FHIR_CATEGORY_SYSTEM: &str = "urn:ehr:document-category";
pub const FHIR_RELATIONSHIP_SYSTEM: &str = "http://hl7.org/fhir/document-relationship-type";
pub const FHIR_READ_TOKEN_EXTENSION: &str = "urn:ehr:ucan-read-token";
pub const FHIR_VALID_UNTIL_EXTENSION: &str = "urn:ehr:valid-until";
pub const IPFS_URL_PREFIX: &str = "ipfs://";

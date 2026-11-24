/// Prefiks multicodec klucza publicznego Ed25519 (`ed25519-pub` 0xed, varint `0xed 0x01`).
pub const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];
pub const DID_KEY_PREFIX: &str = "did:key:z";

pub const CMD_DOC_ISSUE: &str = "/ehr/doc/issue";
pub const CMD_DOC_READ: &str = "/ehr/doc/read";

/// Wersja formatu tokenu i kopert.
pub const ENVELOPE_VERSION: u8 = 1;

/// Pliki magazynu kluczy w katalogu `keys/`.
pub const NODE_KEY_FILE: &str = "node-key.enc";
pub const PGP_SECRET_FILE: &str = "pgp-secret.asc";
pub const PGP_PUBLIC_FILE: &str = "pgp-public.asc";

/// Minimalna długość hasła (znaki).
pub const MIN_PASSWORD_CHARS: usize = 8;
pub const XCHACHA_NONCE_BYTES: usize = 24;
pub const ARGON2_SALT_BYTES: usize = 16;

/// Prefiks tekstowej wizytówki / zaproszenia (`ehr1:` + base64url JSON).
pub const CARD_TEXT_PREFIX: &str = "ehr1:";

use libp2p::identity::Keypair;
use pgp::composed::{SignedPublicKey, SignedSecretKey};
use serde::{Deserialize, Serialize};

/// Podpisany ładunek: dokładne bajty JSON (base64) + podpis Ed25519 klucza węzła nad nimi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed {
    pub v: u8,
    /// DID podpisującego; klucz publiczny jest w samym DID.
    pub signer: String,
    pub payload: String,
    pub signature: String,
}

/// Token: podzbiór pól UCAN 1.0 Delegation. Czas w sekundach Unix UTC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub cmd: String,
    /// CID dokumentów, których dotyczy token; pusta lista = wszystkie dokumenty `sub`.
    pub pol: Vec<String>,
    pub nonce: String,
    pub nbf: u64,
    pub exp: u64,
    /// Dowody w łańcuchu, wbudowane (zamiast CID tokenów jak w UCAN 1.0).
    pub prf: Vec<Signed>,
}

/// Wizytówka lub zaproszenie: DID + klucz OpenPGP + nazwa, podpisane kluczem węzła,
/// czyli powiązanie „ten klucz OpenPGP należy do tego DID”.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardPayload {
    pub did: String,
    pub name: String,
    /// `doctor` albo `patient`; deklaracja wystawcy wizytówki.
    pub role: String,
    pub pgp_public: String,
    /// Adres węzła `/ip4/../tcp/../p2p/<PeerId>` (zaproszenie lekarza).
    pub multiaddr: Option<String>,
    /// Jednorazowy kod zaproszenia (jak link SimpleX); `None` w odpowiedzi pacjenta.
    pub invite: Option<String>,
    pub created_utc: u64,
}

/// Zweryfikowana wizytówka z odczytanym kluczem OpenPGP.
#[derive(Clone, Debug)]
pub struct ContactCard {
    pub payload: CardPayload,
    pub signed: Signed,
    pub pgp: SignedPublicKey,
}

/// Odblokowana tożsamość: klucz węzła (DID, PeerId, podpisy) i klucz OpenPGP.
/// Klucz OpenPGP zostaje zaszyfrowany hasłem także w pamięci; hasło odblokowuje go przy użyciu.
pub struct Identity {
    pub keypair: Keypair,
    pub pgp_secret: SignedSecretKey,
    pub pgp_public: SignedPublicKey,
    pub(crate) password: zeroize::Zeroizing<String>,
}

/// Plik z kluczem węzła zaszyfrowanym hasłem (Argon2id + XChaCha20-Poly1305).
#[derive(Serialize, Deserialize)]
pub(crate) struct EncryptedKeyFile {
    pub(crate) v: u8,
    pub(crate) kdf: String,
    pub(crate) salt: String,
    pub(crate) nonce: String,
    pub(crate) ciphertext: String,
}

/// Kontekst weryfikacji tokenów: zaufani wystawcy (korzenie `/ehr/doc/issue`),
/// lokalna lista unieważnionych `nonce` i bieżący czas UTC.
pub struct Verifier<'a> {
    pub trusted_issuers: &'a [String],
    pub revoked: &'a [String],
    pub now: u64,
}

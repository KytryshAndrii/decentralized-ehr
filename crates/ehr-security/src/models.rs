use libp2p::identity::Keypair;
use pgp::composed::{SignedPublicKey, SignedSecretKey};
use serde::{Deserialize, Serialize};

/// Signed payload: the exact JSON bytes (base64) + an Ed25519 node key signature over them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed {
    pub v: u8,
    /// Signer DID; the public key is inside the DID.
    pub signer: String,
    pub payload: String,
    pub signature: String,
}

/// Token: a subset of the UCAN 1.0 Delegation fields. Times in Unix seconds, UTC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub cmd: String,
    /// Document CIDs the token covers; an empty list = every document of `sub`.
    pub pol: Vec<String>,
    pub nonce: String,
    pub nbf: u64,
    pub exp: u64,
    /// Proofs of the chain, embedded (instead of token CIDs as in UCAN 1.0).
    pub prf: Vec<Signed>,
}

/// Contact card or invitation: DID + OpenPGP key + name, signed with the node key,
/// i.e. the binding "this OpenPGP key belongs to this DID".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardPayload {
    pub did: String,
    pub name: String,
    /// `doctor` or `patient`, as declared by the card issuer.
    pub role: String,
    pub pgp_public: String,
    /// Node address `/ip4/../tcp/../p2p/<PeerId>` (doctor's invitation).
    pub multiaddr: Option<String>,
    /// One-time invitation code (like a SimpleX link); `None` in the patient's reply.
    pub invite: Option<String>,
    pub created_utc: u64,
}

/// Verified contact card with its parsed OpenPGP key.
#[derive(Clone, Debug)]
pub struct ContactCard {
    pub payload: CardPayload,
    pub signed: Signed,
    pub pgp: SignedPublicKey,
}

/// Unlocked identity: node key (DID, PeerId, signatures) and OpenPGP key.
/// The OpenPGP key stays password-encrypted in memory too; the password unlocks it on use.
pub struct Identity {
    pub keypair: Keypair,
    pub pgp_secret: SignedSecretKey,
    pub pgp_public: SignedPublicKey,
    pub(crate) password: zeroize::Zeroizing<String>,
}

/// File with the node key encrypted by the password (Argon2id + XChaCha20-Poly1305).
#[derive(Serialize, Deserialize)]
pub(crate) struct EncryptedKeyFile {
    pub(crate) v: u8,
    pub(crate) kdf: String,
    pub(crate) salt: String,
    pub(crate) nonce: String,
    pub(crate) ciphertext: String,
}

/// Token verification context: trusted issuers (`/ehr/doc/issue` roots),
/// a local list of revoked `nonce` values and the current UTC time.
pub struct Verifier<'a> {
    pub trusted_issuers: &'a [String],
    pub revoked: &'a [String],
    pub now: u64,
}

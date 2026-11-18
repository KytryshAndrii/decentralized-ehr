//! Warstwa bezpieczeństwa EHR nad węzłem IPFS: did:key z klucza węzła, magazyn kluczy
//! chroniony hasłem, OpenPGP (szyfrowanie + podpis), tokeny UCAN (podzbiór 1.0) i wizytówki.
//! Crate'y `ipfs-*` nie zależą od tego crate'a; tu powstają i są czytane zwykłe bajty.
pub mod card;
pub mod consts;
pub mod envelope;
pub mod error;
pub mod identity;
pub mod keystore;
pub mod models;
pub mod pgp;
pub mod ucan;
pub mod utils;

pub use error::SecurityError;
pub use identity::{did_from_public, peer_id_from_did, public_from_did};
pub use models::{CardPayload, ContactCard, Identity, Signed, Token, Verifier};

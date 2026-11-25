//! EHR security layer over the IPFS node: did:key from the node key, password-protected key
//! store, OpenPGP (encryption + signature), UCAN tokens (1.0 subset) and contact cards.
//! The `ipfs-*` crates do not depend on this crate; they only see plain bytes made here.
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

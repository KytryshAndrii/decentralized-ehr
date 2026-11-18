//! Warstwa EHR (M9): pacjent, dokument, manifest i lokalny rejestr CID z nazwami plików.
//! Wymiana plików w standardzie HL7 FHIR R5 (`DocumentReference`), dokumenty szyfrowane
//! i podpisane OpenPGP, uprawnienia UCAN (`ehr-security`). Wspólna logika CLI i UI.
pub mod app;
pub mod consts;
pub mod error;
pub mod fhir;
pub mod models;
pub mod node;
pub mod store;
pub mod utils;
pub(crate) mod wire;

pub use error::EhrError;
pub use models::{
    Category, Contact, DocumentReference, EhrApp, EhrNode, IndexEntry, NewDocument, Person,
    Profile, Role,
};
pub use store::ContactKind;

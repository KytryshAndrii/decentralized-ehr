//! EHR layer (M9): patient, document, manifest and a local CID index with file names.
//! Files are exchanged as HL7 FHIR R5 (`DocumentReference`), documents are encrypted and
//! signed with OpenPGP, permissions are UCAN (`ehr-security`). Shared by the CLI and the UI.
pub mod app;
pub mod consts;
mod doctor;
pub mod error;
pub mod fhir;
pub mod models;
pub mod node;
mod open;
mod patient;
mod replica;
pub mod store;
pub mod utils;
pub(crate) mod wire;

pub use error::EhrError;
pub use models::{
    Battery, Category, Contact, DocumentReference, EhrApp, EhrNode, IndexEntry, NewDocument,
    Person, Profile, Role,
};
pub use store::ContactKind;

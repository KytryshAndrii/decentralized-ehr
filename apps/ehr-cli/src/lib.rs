//! EHR console app. Without an EHR profile (`identity init`) it runs as a plain IPFS node;
//! with a profile it adds the doctor or patient commands (`ehr-core`), the same as in the UI.
mod cli;
mod ehr_commands;
pub mod services;

pub use cli::{CliResult, run};

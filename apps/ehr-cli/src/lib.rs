//! Aplikacja konsolowa EHR. Bez profilu EHR (`identity init`) działa jak dotąd jako węzeł IPFS;
//! z profilem dochodzą komendy lekarza albo pacjenta (`ehr-core`), te same co w UI.
mod cli;
mod ehr_commands;
pub mod services;

pub use cli::{CliResult, run};

//! Patient node: only reads and downloads documents from the doctors in its contacts.
fn main() -> ehr_cli::CliResult<()> {
    ehr_cli::run(ehr_core::Role::Patient)
}

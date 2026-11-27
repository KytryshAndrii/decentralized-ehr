//! Doctor (= facility) node: issues documents, DHT server, inbox and invitations.
fn main() -> ehr_cli::CliResult<()> {
    ehr_cli::run(ehr_core::Role::Doctor)
}

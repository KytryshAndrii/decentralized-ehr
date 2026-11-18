//! Węzeł pacjenta: tylko odczyt i pobranie dokumentów od lekarzy z kontaktów.
fn main() -> ehr_cli::CliResult<()> {
    ehr_cli::run(ehr_core::Role::Patient)
}

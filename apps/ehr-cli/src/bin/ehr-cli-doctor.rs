//! Węzeł lekarza (= placówki): wystawia dokumenty, serwer DHT, skrzynka i zaproszenia.
fn main() -> ehr_cli::CliResult<()> {
    ehr_cli::run(ehr_core::Role::Doctor)
}

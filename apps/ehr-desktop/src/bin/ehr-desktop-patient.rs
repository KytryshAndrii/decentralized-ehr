//! Okno pacjenta: tylko odczyt i pobranie.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    ehr_desktop::run(ehr_core::Role::Patient)
}

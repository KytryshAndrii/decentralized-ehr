//! Patient window: read and download only.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    ehr_desktop::run(ehr_core::Role::Patient)
}

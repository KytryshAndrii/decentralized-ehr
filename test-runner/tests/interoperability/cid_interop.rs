use ipfs_cid::consts::{CODEC_JSON, CODEC_RAW};
use serial_test::serial;
use test_runner::cid_interop_utils::verify_file_interoperability;

// ==========================================
// TESTY PLIKÓW HYPR-TEXT
// ==========================================
#[test]
#[serial]
fn test_kubo_interop_json() {
    verify_file_interoperability(
        "mocks/hypr-text/hl7-fhir-patient-example.json",
        CODEC_JSON,
        "json",
    );
}

#[test]
#[serial]
fn test_kubo_interop_xml() {
    verify_file_interoperability(
        "mocks/hypr-text/hl7-fhir-patient-example.xml",
        CODEC_RAW,
        "raw",
    );
}

#[test]
#[serial]
fn test_kubo_interop_ttl() {
    verify_file_interoperability(
        "mocks/hypr-text/hl7-fhir-patient-example.ttl",
        CODEC_RAW,
        "raw",
    );
}

// ==========================================
// TESTY PLIKÓW GRAFICZNYCH
// ==========================================
#[test]
#[serial]
fn test_kubo_interop_bmp() {
    verify_file_interoperability("mocks/imgs/synthetic-ecg.bmp", CODEC_RAW, "raw");
}

#[test]
#[serial]
fn test_kubo_interop_svg() {
    verify_file_interoperability("mocks/imgs/synthetic-ecg.svg", CODEC_RAW, "raw");
}

#[test]
#[serial]
fn test_kubo_interop_tiff() {
    verify_file_interoperability("mocks/imgs/synthetic-ecg.tiff", CODEC_RAW, "raw");
}

// ==========================================
// TEST PLIKU TXT
// ==========================================
#[test]
#[serial]
fn test_kubo_interop_txt() {
    verify_file_interoperability("mocks/lorem-ipsum.txt", CODEC_RAW, "raw");
}

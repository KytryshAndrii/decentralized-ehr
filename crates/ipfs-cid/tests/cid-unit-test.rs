use ipfs_cid::consts::{CODEC_JSON, CODEC_RAW, SHA2_256, SHA2_512};
use ipfs_cid::error::CidError;
use ipfs_cid::models::{Cid, Multihash};
use ipfs_cid::unit_test_util::generate_mock_cid_string;

// ==========================================
// TESTY OBSŁUGI BŁĘDÓW
// ==========================================

#[test]
fn test_cid_rejects_trailing_bytes() {
    let data = b"test data for trailing bytes";
    let hash = Multihash::compute(data, SHA2_256).unwrap();
    let cid = Cid::new_cid(CODEC_RAW, hash);

    let mut bytes = cid.to_bytes();
    bytes.push(0xFF);

    let result = Cid::decode_from_bytes(&bytes);
    assert_eq!(result, Err(CidError::TrailingBytes));
}

#[test]
fn test_cid_rejects_unsupported_version() {
    let data = b"version test";
    let hash = Multihash::compute(data, SHA2_256).unwrap();
    let mut cid = Cid::new_cid(CODEC_RAW, hash);

    cid.version = 2;
    let bytes = cid.to_bytes();

    let result = Cid::decode_from_bytes(&bytes);
    assert_eq!(result, Err(CidError::UnsupportedVersion(2)));
}

#[test]
fn test_multibase_parse_error_on_invalid_string() {
    let invalid_cid_str = "invalid_string_bafk...";
    let result = Cid::decode_from_string(invalid_cid_str);

    assert!(matches!(result, Err(CidError::MultibaseParseError(_))));
}

// ==========================================
// TESTY KRYPTOGRAFICZNE
// ==========================================

#[test]
fn test_multihash_generation_256() {
    let data = b"hello";
    let hash = Multihash::compute(data, SHA2_256).unwrap();
    assert_eq!(hash.code, SHA2_256);
    assert_eq!(hash.size, 32);
    // Sprawdzenie pierwszych bajtów digestu
    assert_eq!(&hash.digest[0..4], &[0x2c, 0xf2, 0x4d, 0xba]);
}

#[test]
fn test_multihash_generation_512() {
    let data = b"hello";
    let hash = Multihash::compute(data, SHA2_512).unwrap();
    assert_eq!(hash.code, SHA2_512);
    assert_eq!(hash.size, 64);
    // Sprawdzenie pierwszych bajtów digestu
    assert_eq!(&hash.digest[0..4], &[0x9b, 0x71, 0xd2, 0x24]);
}

// ==========================================
// TESTY GENEROWANIA CID DLA RÓŻNYCH PLIKÓW. SHA2-256
// ==========================================

#[test]
fn test_generate_cid_for_json_sha2_256() {
    // Pliki JSON mogą korzystać z dedykowanego multicodec 0x0200
    let cid_str = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.json",
        CODEC_JSON,
        SHA2_256,
    );
    assert!(cid_str.starts_with('b'));
}

#[test]
fn test_generate_cid_for_json_as_raw_sha2_256() {
    // Pliki JSON mogą korzystać z dedykowanego multicodec 0x0200
    let cid_str = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.json",
        CODEC_RAW,
        SHA2_256,
    );
    assert!(cid_str.starts_with('b'));
}

#[test]
fn test_generate_cid_for_xml_and_ttl_sha2_256() {
    // Formaty tekstowe traktowane jako surowe dane
    let xml_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.xml",
        CODEC_RAW,
        SHA2_256,
    );

    assert!(xml_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_ttl_sha2_256() {
    // Formaty tekstowe traktowane jako surowe dane
    let ttl_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.ttl",
        CODEC_RAW,
        SHA2_256,
    );

    assert!(ttl_cid.starts_with('b'));
}

#[test]
fn test_unique_cids_for_differrent_xml_and_ttl_sha2_256() {
    // Formaty tekstowe traktowane jako surowe dane
    let xml_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.xml",
        CODEC_RAW,
        SHA2_256,
    );
    let ttl_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.ttl",
        CODEC_RAW,
        SHA2_256,
    );

    assert_ne!(xml_cid, ttl_cid); // Różna zawartość = różny CID
}

#[test]
fn test_generate_cid_for_tiff_image_sha2_256() {
    let tiff_cid = generate_mock_cid_string(
        "../../test-runner/mocks/imgs/synthetic-ecg.tiff",
        CODEC_RAW,
        SHA2_256,
    );

    assert!(tiff_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_bmp_image_sha2_256() {
    let bmp_cid = generate_mock_cid_string(
        "../../test-runner/mocks/imgs/synthetic-ecg.bmp",
        CODEC_RAW,
        SHA2_256,
    );

    assert!(bmp_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_svg_image_sha2_256() {
    let svg_cid = generate_mock_cid_string(
        "../../test-runner/mocks/imgs/synthetic-ecg.svg",
        CODEC_RAW,
        SHA2_256,
    );

    assert!(svg_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_txt_sha2_256() {
    let txt_cid = generate_mock_cid_string(
        "../../test-runner/mocks/lorem-ipsum.txt",
        CODEC_RAW,
        SHA2_256,
    );
    assert!(txt_cid.starts_with('b'));
}

// ==========================================
// TESTY GENEROWANIA CID DLA RÓŻNYCH PLIKÓW. SHA2-512
// ==========================================

#[test]
fn test_generate_cid_for_json_sha2_512() {
    // Pliki JSON mogą korzystać z dedykowanego multicodec 0x0200
    let cid_str = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.json",
        CODEC_JSON,
        SHA2_512,
    );
    assert!(cid_str.starts_with('b'));
}

#[test]
fn test_generate_cid_for_json_as_raw_sha2_512() {
    // Pliki JSON mogą korzystać z dedykowanego multicodec 0x0200
    let cid_str = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.json",
        CODEC_RAW,
        SHA2_512,
    );
    assert!(cid_str.starts_with('b'));
}

#[test]
fn test_generate_cid_for_xml_and_ttl_sha2_512() {
    // Formaty tekstowe traktowane jako surowe dane
    let xml_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.xml",
        CODEC_RAW,
        SHA2_512,
    );

    assert!(xml_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_ttl_sha2_512() {
    // Formaty tekstowe traktowane jako surowe dane
    let ttl_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.ttl",
        CODEC_RAW,
        SHA2_512,
    );

    assert!(ttl_cid.starts_with('b'));
}

#[test]
fn test_unique_cids_for_differrent_xml_and_ttl_sha2_512() {
    // Formaty tekstowe traktowane jako surowe dane
    let xml_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.xml",
        CODEC_RAW,
        SHA2_512,
    );
    let ttl_cid = generate_mock_cid_string(
        "../../test-runner/mocks/hypr-text/hl7-fhir-patient-example.ttl",
        CODEC_RAW,
        SHA2_512,
    );

    assert_ne!(xml_cid, ttl_cid); // Różna zawartość = różny CID
}

#[test]
fn test_generate_cid_for_tiff_image_sha2_512() {
    let tiff_cid = generate_mock_cid_string(
        "../../test-runner/mocks/imgs/synthetic-ecg.tiff",
        CODEC_RAW,
        SHA2_512,
    );

    assert!(tiff_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_bmp_image_sha2_512() {
    let bmp_cid = generate_mock_cid_string(
        "../../test-runner/mocks/imgs/synthetic-ecg.bmp",
        CODEC_RAW,
        SHA2_512,
    );

    assert!(bmp_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_svg_image_sha2_512() {
    let svg_cid = generate_mock_cid_string(
        "../../test-runner/mocks/imgs/synthetic-ecg.svg",
        CODEC_RAW,
        SHA2_512,
    );

    assert!(svg_cid.starts_with('b'));
}

#[test]
fn test_generate_cid_for_txt_sha2_512() {
    let txt_cid = generate_mock_cid_string(
        "../../test-runner/mocks/lorem-ipsum.txt",
        CODEC_RAW,
        SHA2_512,
    );
    assert!(txt_cid.starts_with('b'));
}

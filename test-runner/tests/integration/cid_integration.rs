use ipfs_cid::consts::{CODEC_JSON, CODEC_RAW, SHA2_256};
use ipfs_cid::models::{Cid, Multihash};
use std::fs;

// ==========================================
// TESTY PEŁNEGO CYKLU ŻYCIA
// ==========================================

#[test]
fn test_full_lifecycle_raw_data() {
    // 1. Inicjalizacja danych wejściowych
    let data = b"Integracyjny test przeplywu danych IPFS";

    // 2. Generowanie wewnętrznych struktur
    let hash = Multihash::compute(data, SHA2_256).expect("Błąd obliczania hasha");
    let original_cid = Cid::new_cid(CODEC_RAW, hash);

    // 3. Transformacja do warstw transportowych (bajty i tekst)
    let binary_format = original_cid.to_bytes();
    let text_format = original_cid.to_string_base32();

    // 4. Rekonstrukcja z warstw transportowych
    let cid_from_bytes = Cid::decode_from_bytes(&binary_format).expect("Błąd dekodowania bajtów");
    let cid_from_text = Cid::decode_from_string(&text_format).expect("Błąd dekodowania multibase");

    // 5. Weryfikacja spójności (wszystkie ścieżki muszą dać ten sam obiekt)
    assert_eq!(original_cid, cid_from_bytes);
    assert_eq!(original_cid, cid_from_text);

    // 6. Walidacja kryptograficzna bloku
    assert!(
        cid_from_text.verify_block(data),
        "Walidacja oryginalnych danych musi zwrócić true"
    );
}

// ==========================================
// TESTY PRZYPADKÓW BRZEGOWYCH I ZABEZPIECZEŃ
// ==========================================

#[test]
fn test_edge_case_corrupted_data_verification() {
    let data = b"Prawidlowa dokumentacja medyczna pacjenta";
    let hash = Multihash::compute(data, SHA2_256).unwrap();
    let cid = Cid::new_cid(CODEC_RAW, hash);

    // Symulujemy dane zmienione w tranzycie (np. przez atakującego lub błąd dysku)
    let mut corrupted_data = data.to_vec();
    corrupted_data[0] = b'X'; // Zmieniamy 'P' na 'X'

    // Struktura CID musi bezwzględnie odrzucić takie dane
    assert!(
        !cid.verify_block(&corrupted_data),
        "CID nie może pozytywnie zweryfikować zmodyfikowanych danych!"
    );
}

#[test]
fn test_multibase_format_agnosticism() {
    let data = b"Dane niezalezne od formatu tekstowego";
    let hash = Multihash::compute(data, SHA2_256).unwrap();
    let cid = Cid::new_cid(CODEC_RAW, hash);

    let binary_data = cid.to_bytes();

    // Kodujemy ten sam identyfikator w dwóch różnych formatach tekstowych
    // Używamy z i b - standardowych prefiksów w IPFS
    let base58btc_str = multibase::encode(multibase::Base::Base58Btc, &binary_data);
    let base32_str = multibase::encode(multibase::Base::Base32Lower, &binary_data);

    assert_ne!(
        base58btc_str, base32_str,
        "Ciągi tekstowe muszą wyglądać inaczej"
    );
    assert!(base58btc_str.starts_with('z'));
    assert!(base32_str.starts_with('b'));

    // Niezależnie od formatu tekstu, deserializacja musi dać ten sam obiekt CID
    let cid_from_base58 = Cid::decode_from_string(&base58btc_str).unwrap();
    let cid_from_base32 = Cid::decode_from_string(&base32_str).unwrap();

    assert_eq!(
        cid_from_base58, cid_from_base32,
        "Zdekodowane obiekty CID muszą być identyczne"
    );
}

// ==========================================
// TESTY END-TO-END Z PLIKAMI ZEWNĘTRZNYMI
// ==========================================

#[test]
fn test_end_to_end_with_json_mock() {
    let file_path = "mocks/hypr-text/hl7-fhir-patient-example.json";
    let file_content =
        fs::read(file_path).expect("Nie udało się odczytać pliku JSON z katalogu mocks");

    let hash = Multihash::compute(&file_content, SHA2_256).unwrap();
    let cid = Cid::new_cid(CODEC_JSON, hash);
    println!(
        "Wygenerowany CID dla pliku JSON: {}",
        cid.to_string_base32()
    );

    let cid_text = cid.to_string_base32();
    let parsed_cid = Cid::decode_from_string(&cid_text).unwrap();

    assert_eq!(parsed_cid.codec, CODEC_JSON);
    assert!(parsed_cid.verify_block(&file_content));
}

pub mod cid;
pub mod consts;
pub mod error;
pub mod models;
pub mod multihash;

#[cfg(test)]
mod tests {
    use super::consts::{CODEC_JSON, CODEC_RAW, SHA2_256};
    use super::error::CidError;
    use super::models::{Cid, Multihash};

    // Ten test weryfikuje pełną interpolację i zgodność z tym co generuje IPFS Kubo (Node)
    #[test]
    fn test_kubo_interoperability_raw_txt() {
        // Dane testowe "hello" odpowiadające Kubo: `echo -n "hello" | ipfs add --cid-version 1`
        let data = b"hello";
        let hash = Multihash::compute(data, SHA2_256).unwrap();
        let cid = Cid::new_v1(CODEC_RAW, hash);

        let base32_str = cid.to_string_base32();

        // bafkrei to typowy prefix Base32 dla CIDv1, raw, sha2-256
        assert_eq!(
            base32_str,
            "bafkreibm6jg3ux5qumhcn2b3flc3tyu6dmlb4xa7u5bf44yegnrjhc4yeq"
        );

        // Test dekodowania (Text -> Obiekt Rust)
        let decoded_cid = Cid::from_string(&base32_str).unwrap();
        assert_eq!(cid, decoded_cid);
    }

    #[test]
    fn test_dummy_json_cid() {
        // Atrapa pliku JSON
        let json_data = br#"{"patient_id": "12345", "status": "healthy"}"#;
        let hash = Multihash::compute(json_data, SHA2_256).unwrap();

        // Używamy dedykowanego kodeku JSON
        let cid = Cid::new_v1(CODEC_JSON, hash);
        let cid_str = cid.to_string_base32();

        let parsed = Cid::from_string(&cid_str).unwrap();
        assert_eq!(parsed.codec, CODEC_JSON);
    }

    #[test]
    fn test_rejects_trailing_bytes() {
        let data = b"test";
        let hash = Multihash::compute(data, SHA2_256).unwrap();
        let cid = Cid::new_v1(CODEC_RAW, hash);

        let mut bytes = cid.to_bytes();
        // Symulujemy zepsuty pakiet, dodając złośliwy/błędny bajt na końcu
        bytes.push(0xFF);

        let result = Cid::from_bytes(&bytes);
        assert_eq!(result, Err(CidError::TrailingBytes));
    }
}

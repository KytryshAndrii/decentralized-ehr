use crate::consts::CID_V1;
use crate::error::CidError;
use crate::models::Cid;
use crate::models::Multihash;

impl Cid {
    /// Tworzy nowy obiekt CIDv1.
    pub fn new_cid(codec: u64, hash: Multihash) -> Self {
        Self {
            version: CID_V1,
            codec,
            multihash: hash,
        }
    }

    /// Serializuje CID do surowej postaci binarnej (używając unsigned varint).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut varint_buf = unsigned_varint::encode::u64_buffer();

        buf.extend_from_slice(unsigned_varint::encode::u64(self.version, &mut varint_buf));
        buf.extend_from_slice(unsigned_varint::encode::u64(self.codec, &mut varint_buf));
        buf.extend_from_slice(unsigned_varint::encode::u64(
            self.multihash.code,
            &mut varint_buf,
        ));
        buf.extend_from_slice(unsigned_varint::encode::u64(
            self.multihash.size as u64,
            &mut varint_buf,
        ));
        buf.extend_from_slice(&self.multihash.digest);

        buf
    }

    /// Dekoduje strukturę CID z postaci binarnej.
    /// Zgodnie ze specyfikacją, funkcja odrzuca nadmiarowe bajty (TrailingBytes).
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, CidError> {
        let (version, rem) =
            unsigned_varint::decode::u64(bytes).map_err(|_| CidError::VarintDecodeError)?;

        if version != CID_V1 {
            return Err(CidError::UnsupportedVersion(version));
        }

        let (codec, rem) =
            unsigned_varint::decode::u64(rem).map_err(|_| CidError::VarintDecodeError)?;
        let (hash_code, rem) =
            unsigned_varint::decode::u64(rem).map_err(|_| CidError::VarintDecodeError)?;
        let (hash_size, rem) =
            unsigned_varint::decode::u64(rem).map_err(|_| CidError::VarintDecodeError)?;

        let hash_size = hash_size as usize;
        if rem.len() < hash_size {
            return Err(CidError::VarintDecodeError);
        }

        let digest = rem[..hash_size].to_vec();

        // Zgodnie z MUST specyfikacji: odrzucamy jeśli zostały jakieś śmieciowe bajty
        if rem.len() > hash_size {
            return Err(CidError::TrailingBytes);
        }

        Ok(Self {
            version,
            codec,
            multihash: Multihash {
                code: hash_code,
                size: hash_size as u8,
                digest,
            },
        })
    }

    /// Konwertuje CID na reprezentację tekstową Base32.
    pub fn to_string_base32(&self) -> String {
        multibase::encode(multibase::Base::Base32Lower, self.to_bytes())
    }

    /// Dekoduje CID ze stringa zapisanego w dowolnym formacie Multibase.
    pub fn decode_from_string(cid_str: &str) -> Result<Self, CidError> {
        let (_, bytes) =
            multibase::decode(cid_str).map_err(|e| CidError::MultibaseParseError(e.to_string()))?;

        Self::decode_from_bytes(&bytes)
    }

    /// Weryfikuje, czy podane surowe dane po zahashowaniu pasują do tego CID.
    /// Zwraca `true`, jeśli dane są autentyczne i zgodne z identyfikatorem.
    pub fn verify_block(&self, data: &[u8]) -> bool {
        match Multihash::compute(data, self.multihash.code) {
            Ok(computed_hash) => computed_hash.digest == self.multihash.digest,
            Err(_) => {
                // Zwracamy false w przypadku nieobsługiwanego algorytmu lub błędu
                false
            }
        }
    }
    /// Tworzy nowy identyfikator CID bezpośrednio z dostarczonych bajtów danych.
    pub fn generate_from_data(data: &[u8], codec: u64, hash_algo: u64) -> Result<Self, CidError> {
        let hash = Multihash::compute(data, hash_algo)
            .map_err(|_| CidError::UnsupportedHashAlgorithm(hash_algo))?;

        let cid = Self::new_cid(codec, hash);

        eprintln!(
            "[ipfs-cid] CREATED version={} codec={:#x} hash={:#x} size={}B cid={}",
            cid.version,
            cid.codec,
            cid.multihash.code,
            data.len(),
            cid.to_string_base32()
        );

        Ok(cid)
    }
}

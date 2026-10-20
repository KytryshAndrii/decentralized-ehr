use crate::consts::{SHA2_256, SHA2_512};
use crate::error::CidError;
use crate::models::Multihash;
use sha2::{Digest, Sha256, Sha512};

impl Multihash {
    /// Generuje Multihash z podanych bajtów używając wybranego algorytmu.
    ///
    /// # Argumenty
    /// * `data` - Surowe bajty do zahashowania.
    /// * `code` - Kod algorytmu (np. `SHA2_256``).
    pub fn compute(data: &[u8], code: u64) -> Result<Self, CidError> {
        let digest = match code {
            SHA2_256 => {
                let mut hasher = Sha256::new();
                hasher.update(data);
                hasher.finalize().to_vec()
            }
            SHA2_512 => {
                let mut hasher = Sha512::new();
                hasher.update(data);
                hasher.finalize().to_vec()
            }
            _ => return Err(CidError::UnsupportedHashAlgorithm(code)),
        };

        Ok(Multihash {
            code,
            size: digest.len() as u8,
            digest,
        })
    }
}

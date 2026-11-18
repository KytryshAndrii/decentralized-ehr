use crate::consts::ENVELOPE_VERSION;
use crate::error::SecurityError;
use crate::identity::{did_from_public, public_from_did};
use crate::models::Signed;
use crate::utils::{b64, unb64};
use libp2p::identity::Keypair;
use serde::{Serialize, de::DeserializeOwned};

impl Signed {
    pub fn sign<T: Serialize>(keypair: &Keypair, value: &T) -> Result<Self, SecurityError> {
        let payload = serde_json::to_vec(value)?;
        let signature = keypair
            .sign(&payload)
            .map_err(|error| SecurityError::Key(error.to_string()))?;
        Ok(Self {
            v: ENVELOPE_VERSION,
            signer: did_from_public(&keypair.public())?,
            payload: b64(&payload),
            signature: b64(&signature),
        })
    }

    /// Sprawdza podpis kluczem z `signer` i zwraca ładunek. Kto podpisał, mówi `self.signer`.
    pub fn open<T: DeserializeOwned>(&self) -> Result<T, SecurityError> {
        if self.v != ENVELOPE_VERSION {
            return Err(SecurityError::Encoding(format!(
                "envelope version {}",
                self.v
            )));
        }
        let payload = unb64(&self.payload)?;
        let signature = unb64(&self.signature)?;
        if !public_from_did(&self.signer)?.verify(&payload, &signature) {
            return Err(SecurityError::BadSignature);
        }
        Ok(serde_json::from_slice(&payload)?)
    }

    /// Jak `open`, ale podpisać musiał wskazany DID.
    pub fn open_from<T: DeserializeOwned>(&self, signer: &str) -> Result<T, SecurityError> {
        if self.signer != signer {
            return Err(SecurityError::BadSignature);
        }
        self.open()
    }
}

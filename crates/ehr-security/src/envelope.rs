use crate::consts::ENVELOPE_VERSION;
use crate::error::SecurityError;
use crate::identity::{did_from_public, public_from_did};
use crate::models::Signed;
use crate::utils::{base64_decode, base64_encode};
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
            payload: base64_encode(&payload),
            signature: base64_encode(&signature),
        })
    }

    /// Checks the signature with the key in `signer` and returns the payload; `self.signer` says who signed.
    pub fn open<T: DeserializeOwned>(&self) -> Result<T, SecurityError> {
        if self.v != ENVELOPE_VERSION {
            return Err(SecurityError::Encoding(format!(
                "envelope version {}",
                self.v
            )));
        }
        let payload = base64_decode(&self.payload)?;
        let signature = base64_decode(&self.signature)?;
        if !public_from_did(&self.signer)?.verify(&payload, &signature) {
            return Err(SecurityError::BadSignature);
        }
        Ok(serde_json::from_slice(&payload)?)
    }

    /// Like `open`, but the given DID must be the signer.
    pub fn open_from<T: DeserializeOwned>(&self, signer: &str) -> Result<T, SecurityError> {
        if self.signer != signer {
            return Err(SecurityError::BadSignature);
        }
        self.open()
    }
}

//! Contact cards and invitations as text `ehr1:<base64url(envelope JSON)>`.
//! The envelope is signed with the node key, so it binds the OpenPGP key, name and address to the DID.
use crate::consts::CARD_TEXT_PREFIX;
use crate::error::SecurityError;
use crate::identity::peer_id_from_did;
use crate::models::{CardPayload, ContactCard, Identity, Signed};
use crate::pgp::parse_public_key;
use crate::utils::{base64_decode, base64_encode};

impl ContactCard {
    pub fn create(identity: &Identity, payload: CardPayload) -> Result<Self, SecurityError> {
        if payload.did != identity.did() {
            return Err(SecurityError::InvalidDid(payload.did));
        }
        let signed = Signed::sign(&identity.keypair, &payload)?;
        Self::verify(signed)
    }

    /// Signature matching `did`, a valid OpenPGP key, an address ending with the PeerId of the DID.
    pub fn verify(signed: Signed) -> Result<Self, SecurityError> {
        let payload: CardPayload = signed.open()?;
        if payload.did != signed.signer {
            return Err(SecurityError::BadSignature);
        }
        if let Some(address) = &payload.multiaddr {
            let peer = peer_id_from_did(&payload.did)?.to_string();
            if !address.ends_with(&format!("/p2p/{peer}")) {
                return Err(SecurityError::Encoding(
                    "address does not belong to the DID".into(),
                ));
            }
        }
        let pgp = parse_public_key(&payload.pgp_public)?;
        Ok(Self {
            payload,
            signed,
            pgp,
        })
    }

    pub fn to_text(&self) -> Result<String, SecurityError> {
        Ok(format!(
            "{CARD_TEXT_PREFIX}{}",
            base64_encode(&serde_json::to_vec(&self.signed)?)
        ))
    }

    pub fn from_text(text: &str) -> Result<Self, SecurityError> {
        let encoded = text
            .trim()
            .strip_prefix(CARD_TEXT_PREFIX)
            .ok_or_else(|| SecurityError::Encoding("not an EHR card".into()))?;
        Self::verify(serde_json::from_slice(&base64_decode(encoded)?)?)
    }
}

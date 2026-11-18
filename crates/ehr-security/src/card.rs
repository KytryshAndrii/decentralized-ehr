//! Wizytówki i zaproszenia jako tekst `ehr1:<base64url(JSON koperty)>`.
//! Koperta jest podpisana kluczem węzła, więc wiąże klucz OpenPGP, nazwę i adres z DID.
use crate::consts::CARD_TEXT_PREFIX;
use crate::error::SecurityError;
use crate::identity::peer_id_from_did;
use crate::models::{CardPayload, ContactCard, Identity, Signed};
use crate::pgp::parse_public_key;
use crate::utils::{b64, unb64};

impl ContactCard {
    pub fn create(identity: &Identity, payload: CardPayload) -> Result<Self, SecurityError> {
        if payload.did != identity.did() {
            return Err(SecurityError::InvalidDid(payload.did));
        }
        let signed = Signed::sign(&identity.keypair, &payload)?;
        Self::verify(signed)
    }

    /// Podpis zgodny z `did`, poprawny klucz OpenPGP, adres kończący się PeerId z DID.
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
            b64(&serde_json::to_vec(&self.signed)?)
        ))
    }

    pub fn from_text(text: &str) -> Result<Self, SecurityError> {
        let encoded = text
            .trim()
            .strip_prefix(CARD_TEXT_PREFIX)
            .ok_or_else(|| SecurityError::Encoding("not an EHR card".into()))?;
        Self::verify(serde_json::from_slice(&unb64(encoded)?)?)
    }
}

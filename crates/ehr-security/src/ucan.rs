//! Minimalny podzbiór UCAN 1.0 (nazwy pól jak w specyfikacji Delegation), bez gwarancji
//! zgodności z innymi implementacjami UCAN. Podpisy: Ed25519 klucza węzła (`libp2p-identity`).
//! Reguła pracy: pacjent tylko czyta; token `read` jest ważny wyłącznie, gdy jego wystawca
//! ma własny łańcuch `issue` od zaufanego korzenia (lekarz = placówka, token wystawiony sobie).
use crate::consts::{CMD_DOC_ISSUE, CMD_DOC_READ};
use crate::error::SecurityError;
use crate::models::{Identity, Signed, Token, Verifier};
use crate::utils::random_id;

impl Token {
    /// Korzeń uprawnienia lekarza: `iss = aud = sub` = DID węzła lekarza.
    pub fn issue_root(identity: &Identity, now: u64, exp: u64) -> Result<Signed, SecurityError> {
        let did = identity.did();
        Signed::sign(
            &identity.keypair,
            &Token {
                iss: did.clone(),
                aud: did.clone(),
                sub: did,
                cmd: CMD_DOC_ISSUE.into(),
                pol: Vec::new(),
                nonce: random_id(),
                nbf: now,
                exp,
                prf: Vec::new(),
            },
        )
    }

    /// Lekarz nadaje pacjentowi odczyt jednego dokumentu (`pol` = CID szyfrogramu).
    pub fn delegate_read(
        identity: &Identity,
        issue_root: &Signed,
        patient_did: &str,
        document_cid: &str,
        now: u64,
        exp: u64,
    ) -> Result<Signed, SecurityError> {
        Signed::sign(
            &identity.keypair,
            &Token {
                iss: identity.did(),
                aud: patient_did.into(),
                sub: patient_did.into(),
                cmd: CMD_DOC_READ.into(),
                pol: vec![document_cid.into()],
                nonce: random_id(),
                nbf: now,
                exp,
                prf: vec![issue_root.clone()],
            },
        )
    }
}

impl Verifier<'_> {
    /// Podpis zgodny z `iss`, czas, lista unieważnień.
    fn open(&self, signed: &Signed) -> Result<Token, SecurityError> {
        let token: Token = signed.open()?;
        if signed.signer != token.iss {
            return Err(SecurityError::BadSignature);
        }
        if self.now < token.nbf || self.now >= token.exp {
            return Err(SecurityError::TokenRejected("expired or not yet valid"));
        }
        if self.revoked.contains(&token.nonce) {
            return Err(SecurityError::TokenRejected("revoked"));
        }
        Ok(token)
    }

    /// Korzeń `/ehr/doc/issue` wystawiony samemu sobie przez zaufanego wystawcę.
    pub fn verify_issue(&self, signed: &Signed) -> Result<Token, SecurityError> {
        let token = self.open(signed)?;
        if token.cmd != CMD_DOC_ISSUE {
            return Err(SecurityError::TokenRejected("not an issue token"));
        }
        if token.aud != token.iss || token.sub != token.iss {
            return Err(SecurityError::TokenRejected(
                "issue root must be self-issued",
            ));
        }
        if !self.trusted_issuers.contains(&token.iss) {
            return Err(SecurityError::TokenRejected("issuer is not trusted"));
        }
        Ok(token)
    }

    /// Odczyt dokumentu `document_cid` przez `audience`.
    pub fn verify_read(
        &self,
        signed: &Signed,
        audience: &str,
        document_cid: &str,
    ) -> Result<Token, SecurityError> {
        let token = self.open(signed)?;
        if token.cmd != CMD_DOC_READ {
            return Err(SecurityError::TokenRejected("command not delegated"));
        }
        if token.aud != audience {
            return Err(SecurityError::TokenRejected(
                "token is for another audience",
            ));
        }
        if !token.pol.is_empty() && !token.pol.iter().any(|cid| cid == document_cid) {
            return Err(SecurityError::TokenRejected("document outside the policy"));
        }
        // Ciągłość łańcucha: korzeń `issue` należy do wystawcy tego tokenu.
        let chained = token.prf.iter().any(|proof| {
            self.verify_issue(proof)
                .is_ok_and(|root| root.aud == token.iss)
        });
        if !chained {
            return Err(SecurityError::TokenRejected("issuer has no issue chain"));
        }
        Ok(token)
    }
}

//! Opening a document: read permission check, fetch, decrypt and signature check.
use crate::error::EhrError;
use crate::models::{EhrNode, IndexEntry, Role};
use crate::node::blocking;
use crate::store::ContactKind;
use ehr_security::ContactCard;
use ehr_security::pgp::decrypt_and_verify;
use std::path::Path;

impl EhrNode {
    /// Reading: the patient needs a valid token (20 years) from a doctor in its contacts, the
    /// doctor reads only its own documents. The issuer's OpenPGP signature is always checked.
    pub async fn open_document(&self, entry: &IndexEntry) -> Result<Vec<u8>, EhrError> {
        let issuer_pgp = match self.app.role {
            Role::Patient => {
                self.verify_read(entry)?;
                let issuer = entry.issuer_did.clone();
                let doctor = blocking(&self.app, move |app| {
                    app.contact(ContactKind::Doctor, &issuer)?
                        .ok_or_else(|| EhrError::NotFound(format!("doctor {issuer}")))
                })
                .await?;
                ContactCard::from_text(&doctor.card)?.pgp
            }
            Role::Doctor if entry.issuer_did == self.app.profile.did => {
                self.app.identity.pgp_public.clone()
            }
            Role::Doctor => return Err(EhrError::Rejected("document of another issuer".into())),
        };
        let sealed = self.read_file(&entry.doc_cid).await?;
        Ok(decrypt_and_verify(
            &sealed,
            &self.app.identity,
            &issuer_pgp,
        )?)
    }

    /// Writes the decrypted file to the path given by the user (never overwrites).
    pub async fn save_document(&self, entry: &IndexEntry, output: &Path) -> Result<(), EhrError> {
        if !output.is_absolute() {
            return Err(EhrError::Invalid("output path must be absolute".into()));
        }
        let bytes = self.open_document(entry).await?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        file.write_all(&bytes)?;
        println!(
            "[DOC] SAVED name=\"{}\" path={}",
            entry.file_name,
            output.display()
        );
        Ok(())
    }
}

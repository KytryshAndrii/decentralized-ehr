//! Contacts (`contacts.leveldb`) and the document index (`index.leveldb`): separate LevelDB
//! folders with their own lock (DEC8), opened for one operation like the blockstore.
use crate::consts::{
    CONTACTS_DB_DIR, DOCTOR_PREFIX, INDEX_DB_DIR, INDEX_PREFIX, INVITE_PREFIX, PATIENT_PREFIX,
    PENDING_PREFIX,
};
use crate::error::EhrError;
use crate::models::{Contact, EhrApp, IndexEntry, Invite};
use ipfs_store::Datastore;
use ipfs_store::models::LevelDbDatastore;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactKind {
    Patient,
    Doctor,
    /// The patient answered the invitation; the doctor has not accepted yet.
    Pending,
}

impl ContactKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Patient => PATIENT_PREFIX,
            Self::Doctor => DOCTOR_PREFIX,
            Self::Pending => PENDING_PREFIX,
        }
    }
}

fn index_key(patient: &str, issuer: &str, seq: u64) -> Vec<u8> {
    let mut key = format!("{INDEX_PREFIX}{patient}/{issuer}/").into_bytes();
    key.extend_from_slice(&seq.to_be_bytes());
    key
}

fn decode_all<T: DeserializeOwned>(entries: Vec<(Vec<u8>, Vec<u8>)>) -> Result<Vec<T>, EhrError> {
    entries
        .into_iter()
        .map(|(_, value)| Ok(serde_json::from_slice(&value)?))
        .collect()
}

impl EhrApp {
    fn database<T>(
        &self,
        name: &str,
        work: impl FnOnce(&LevelDbDatastore) -> Result<T, EhrError>,
    ) -> Result<T, EhrError> {
        let _guard = self
            .db_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&LevelDbDatastore::open(self.data_dir.join(name))?)
    }

    fn put_json(
        database: &LevelDbDatastore,
        key: &[u8],
        value: &impl Serialize,
    ) -> Result<(), EhrError> {
        Ok(database.put(key, &serde_json::to_vec(value)?)?)
    }

    pub(crate) fn save_contact(
        &self,
        kind: ContactKind,
        contact: &Contact,
    ) -> Result<(), EhrError> {
        self.database(CONTACTS_DB_DIR, |database| {
            Self::put_json(
                database,
                format!("{}{}", kind.prefix(), contact.did).as_bytes(),
                contact,
            )
        })
    }

    pub(crate) fn remove_contact(&self, kind: ContactKind, did: &str) -> Result<(), EhrError> {
        self.database(CONTACTS_DB_DIR, |database| {
            Ok(database.delete(format!("{}{did}", kind.prefix()).as_bytes())?)
        })
    }

    pub fn contact(&self, kind: ContactKind, did: &str) -> Result<Option<Contact>, EhrError> {
        self.database(CONTACTS_DB_DIR, |database| {
            database
                .get(format!("{}{did}", kind.prefix()).as_bytes())?
                .map(|bytes| Ok(serde_json::from_slice(&bytes)?))
                .transpose()
        })
    }

    pub fn contacts(&self, kind: ContactKind) -> Result<Vec<Contact>, EhrError> {
        let mut contacts: Vec<Contact> = self.database(CONTACTS_DB_DIR, |database| {
            decode_all(database.entries_with_prefix(kind.prefix().as_bytes())?)
        })?;
        contacts.sort_by(|left, right| left.person.display_name().cmp(right.person.display_name()));
        Ok(contacts)
    }

    pub(crate) fn save_invite(&self, invite: &Invite) -> Result<(), EhrError> {
        self.database(CONTACTS_DB_DIR, |database| {
            Self::put_json(
                database,
                format!("{INVITE_PREFIX}{}", invite.code).as_bytes(),
                invite,
            )
        })
    }

    /// Single use: the code is removed on the first use, also when it expired.
    pub(crate) fn take_invite(&self, code: &str, now: u64) -> Result<bool, EhrError> {
        self.database(CONTACTS_DB_DIR, |database| {
            let key = format!("{INVITE_PREFIX}{code}");
            let Some(bytes) = database.get(key.as_bytes())? else {
                return Ok(false);
            };
            database.delete(key.as_bytes())?;
            let invite: Invite = serde_json::from_slice(&bytes)?;
            Ok(now < invite.expires_utc)
        })
    }

    pub(crate) fn save_entry(&self, entry: &IndexEntry) -> Result<(), EhrError> {
        self.database(INDEX_DB_DIR, |database| {
            let key = index_key(&entry.patient_did, &entry.issuer_did, entry.seq);
            Self::put_json(database, &key, entry)
        })
    }

    /// Doctor: the next number for the patient is assigned and saved in one database session.
    pub(crate) fn save_new_entry(&self, mut entry: IndexEntry) -> Result<IndexEntry, EhrError> {
        self.database(INDEX_DB_DIR, |database| {
            let prefix = format!("{INDEX_PREFIX}{}/{}/", entry.patient_did, entry.issuer_did);
            let last = database.entries_with_prefix(prefix.as_bytes())?.pop();
            entry.seq = match last {
                Some((_, value)) => serde_json::from_slice::<IndexEntry>(&value)?.seq + 1,
                None => 1,
            };
            let key = index_key(&entry.patient_did, &entry.issuer_did, entry.seq);
            Self::put_json(database, &key, &entry)?;
            Ok(entry)
        })
    }

    /// All document versions of the patient (from every issuer), in key order.
    pub(crate) fn all_entries(&self, patient: &str) -> Result<Vec<IndexEntry>, EhrError> {
        self.database(INDEX_DB_DIR, |database| {
            decode_all(
                database.entries_with_prefix(format!("{INDEX_PREFIX}{patient}/").as_bytes())?,
            )
        })
    }

    /// Entries of one issuer with a number above `since` (the inbox answer).
    pub(crate) fn entries_since(
        &self,
        patient: &str,
        issuer: &str,
        since: u64,
    ) -> Result<Vec<IndexEntry>, EhrError> {
        let prefix = format!("{INDEX_PREFIX}{patient}/{issuer}/");
        let entries: Vec<IndexEntry> = self.database(INDEX_DB_DIR, |database| {
            decode_all(database.entries_with_prefix(prefix.as_bytes())?)
        })?;
        Ok(entries
            .into_iter()
            .filter(|entry| entry.seq > since)
            .collect())
    }

    pub(crate) fn last_seq(&self, patient: &str, issuer: &str) -> Result<u64, EhrError> {
        Ok(self
            .entries_since(patient, issuer, 0)?
            .last()
            .map_or(0, |entry| entry.seq))
    }

    /// List to show: the latest version of each document, newest first.
    /// `patient = None` on the patient side means its own documents.
    pub fn documents(&self, patient: Option<&str>) -> Result<Vec<IndexEntry>, EhrError> {
        let patient = patient.unwrap_or(&self.profile.did);
        let mut latest: HashMap<(String, String), IndexEntry> = HashMap::new();
        for entry in self.all_entries(patient)? {
            let key = (entry.issuer_did.clone(), entry.doc_id.clone());
            if latest
                .get(&key)
                .is_none_or(|known| known.version < entry.version)
            {
                latest.insert(key, entry);
            }
        }
        let mut documents: Vec<IndexEntry> = latest.into_values().collect();
        documents.sort_by(|left, right| {
            (right.created_utc, &right.file_name, right.seq).cmp(&(
                left.created_utc,
                &left.file_name,
                left.seq,
            ))
        });
        Ok(documents)
    }
}

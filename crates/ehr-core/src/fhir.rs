//! HL7 FHIR R5: `Patient` / `Practitioner` (contacts) and `DocumentReference` (file manifest).
use crate::consts::{
    FHIR_CATEGORY_SYSTEM, FHIR_DID_SYSTEM, FHIR_DOC_ID_SYSTEM, FHIR_READ_TOKEN_EXTENSION,
    FHIR_RELATIONSHIP_SYSTEM, FHIR_VALID_UNTIL_EXTENSION, IPFS_URL_PREFIX,
};
use crate::error::EhrError;
use crate::models::{
    Attachment, Category, CodeableConcept, CodeableReference, Coding, Content, DocumentReference,
    Extension, HumanName, Identifier, IndexEntry, Person, Reference, RelatesTo, Role,
};
use crate::utils::iso8601;

impl Category {
    pub const ALL: [Category; 4] = [Self::Lab, Self::Imaging, Self::Discharge, Self::Other];

    pub fn code(self) -> &'static str {
        match self {
            Self::Lab => "lab",
            Self::Imaging => "imaging",
            Self::Discharge => "discharge",
            Self::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lab => "Wyniki badań",
            Self::Imaging => "Obrazowe",
            Self::Discharge => "Wypisy i zalecenia",
            Self::Other => "Inne",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.code() == code)
    }
}

fn did_identifier(did: &str) -> Identifier {
    Identifier {
        system: FHIR_DID_SYSTEM.into(),
        value: did.into(),
    }
}

impl Person {
    /// `Patient` for a patient contact, `Practitioner` for a doctor contact.
    pub fn new(role: Role, did: &str, name: &str) -> Self {
        let name = name.trim();
        let (given, family) = match name.rsplit_once(' ') {
            Some((given, family)) => (vec![given.to_owned()], Some(family.to_owned())),
            None => (Vec::new(), None),
        };
        Self {
            resource_type: match role {
                Role::Patient => "Patient",
                Role::Doctor => "Practitioner",
            }
            .into(),
            identifier: vec![did_identifier(did)],
            active: true,
            name: vec![HumanName {
                text: name.into(),
                family,
                given,
            }],
        }
    }

    pub fn display_name(&self) -> &str {
        self.name
            .first()
            .map(|name| name.text.as_str())
            .unwrap_or("")
    }
}

fn extension<'a>(document: &'a DocumentReference, url: &str) -> Option<&'a Extension> {
    document
        .extension
        .iter()
        .find(|extension| extension.url == url)
}

fn reference_did(reference: &Reference) -> Option<&str> {
    reference
        .identifier
        .as_ref()
        .filter(|identifier| identifier.system == FHIR_DID_SYSTEM)
        .map(|identifier| identifier.value.as_str())
}

impl DocumentReference {
    /// Manifest of a document version (without `seq`, which the index assigns).
    pub fn from_entry(entry: &IndexEntry) -> Result<Self, EhrError> {
        Ok(Self {
            resource_type: "DocumentReference".into(),
            extension: vec![
                Extension {
                    url: FHIR_READ_TOKEN_EXTENSION.into(),
                    value_string: Some(serde_json::to_string(&entry.read_token)?),
                    value_date_time: None,
                },
                Extension {
                    url: FHIR_VALID_UNTIL_EXTENSION.into(),
                    value_string: None,
                    value_date_time: Some(iso8601(entry.valid_until_utc)),
                },
            ],
            identifier: vec![Identifier {
                system: FHIR_DOC_ID_SYSTEM.into(),
                value: entry.doc_id.clone(),
            }],
            version: entry.version.to_string(),
            status: "current".into(),
            doc_status: "final".into(),
            category: vec![CodeableConcept {
                coding: vec![Coding {
                    system: FHIR_CATEGORY_SYSTEM.into(),
                    code: entry.category.code().into(),
                    display: Some(entry.category.label().into()),
                }],
            }],
            subject: Reference {
                identifier: Some(did_identifier(&entry.patient_did)),
                display: None,
            },
            date: iso8601(entry.created_utc),
            author: vec![Reference {
                identifier: Some(did_identifier(&entry.issuer_did)),
                display: Some(entry.issuer_name.clone()),
            }],
            relates_to: entry
                .prev_manifest_cid
                .iter()
                .map(|prev| RelatesTo {
                    code: CodeableConcept {
                        coding: vec![Coding {
                            system: FHIR_RELATIONSHIP_SYSTEM.into(),
                            code: "replaces".into(),
                            display: None,
                        }],
                    },
                    target: CodeableReference {
                        reference: Reference {
                            identifier: Some(Identifier {
                                system: FHIR_DID_SYSTEM.into(),
                                value: format!("{IPFS_URL_PREFIX}{prev}"),
                            }),
                            display: None,
                        },
                    },
                })
                .collect(),
            content: vec![Content {
                attachment: Attachment {
                    content_type: entry.media_type.clone(),
                    url: format!("{IPFS_URL_PREFIX}{}", entry.doc_cid),
                    size: entry.size.to_string(),
                    title: entry.file_name.clone(),
                    creation: iso8601(entry.created_utc),
                },
            }],
        })
    }

    /// Required R5 fields (`status`, `content`) and the fields needed to open the document.
    /// The manifest read from the doctor becomes an entry of the patient's index.
    pub fn to_entry(
        &self,
        seq: u64,
        manifest_cid: &str,
        created_utc: u64,
        valid_until_utc: u64,
    ) -> Result<IndexEntry, EhrError> {
        let invalid = |what: &str| EhrError::Invalid(format!("DocumentReference: {what}"));
        if self.resource_type != "DocumentReference" || self.status != "current" {
            return Err(invalid("resourceType/status"));
        }
        let [content] = self.content.as_slice() else {
            return Err(invalid("exactly one content"));
        };
        let doc_cid = content
            .attachment
            .url
            .strip_prefix(IPFS_URL_PREFIX)
            .ok_or_else(|| invalid("attachment.url must be ipfs://"))?;
        let author = self.author.first().ok_or_else(|| invalid("author"))?;
        let token = extension(self, FHIR_READ_TOKEN_EXTENSION)
            .and_then(|extension| extension.value_string.as_deref())
            .ok_or_else(|| invalid("read token extension"))?;
        let category = self
            .category
            .first()
            .and_then(|concept| concept.coding.first())
            .and_then(|coding| Category::from_code(&coding.code))
            .unwrap_or(Category::Other);
        Ok(IndexEntry {
            seq,
            doc_id: self
                .identifier
                .iter()
                .find(|identifier| identifier.system == FHIR_DOC_ID_SYSTEM)
                .map(|identifier| identifier.value.clone())
                .ok_or_else(|| invalid("document identifier"))?,
            version: self.version.parse().map_err(|_| invalid("version"))?,
            file_name: content.attachment.title.clone(),
            media_type: content.attachment.content_type.clone(),
            size: content
                .attachment
                .size
                .parse()
                .map_err(|_| invalid("size"))?,
            category,
            manifest_cid: manifest_cid.into(),
            doc_cid: doc_cid.into(),
            patient_did: reference_did(&self.subject)
                .ok_or_else(|| invalid("subject"))?
                .into(),
            issuer_did: reference_did(author)
                .ok_or_else(|| invalid("author"))?
                .into(),
            issuer_name: author.display.clone().unwrap_or_default(),
            created_utc,
            valid_until_utc,
            prev_manifest_cid: self.relates_to.first().and_then(|relation| {
                relation
                    .target
                    .reference
                    .identifier
                    .as_ref()
                    .and_then(|identifier| identifier.value.strip_prefix(IPFS_URL_PREFIX))
                    .map(str::to_owned)
            }),
            read_token: serde_json::from_str(token)?,
        })
    }
}

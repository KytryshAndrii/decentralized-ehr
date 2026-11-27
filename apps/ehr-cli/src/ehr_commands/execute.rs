//! Executes one EHR command line: maps the words to `ehr-core` methods and prints the result.
use super::{print_profile, take_option};
use crate::cli::{CliResult, binary_name};
use ehr_core::utils::{date_text, size_text};
use ehr_core::{
    Category, ContactKind, DocumentReference, EhrApp, EhrNode, IndexEntry, NewDocument, Role,
};
use ipfs_network::NetworkConfig;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn require_node(node: Option<&EhrNode>) -> CliResult<&EhrNode> {
    node.ok_or_else(|| "this command needs the running node: use it in `node start`".into())
}

fn absolute(path: &str) -> CliResult<PathBuf> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(format!("path must be absolute: {}", path.display()).into());
    }
    Ok(path)
}

/// Documents of the list the user sees: the doctor those of the given patient, the patient its own.
fn documents(app: &EhrApp, words: &mut Vec<String>) -> CliResult<Vec<IndexEntry>> {
    match app.role() {
        Role::Doctor => {
            let query = take_option(words, "--patient")?.ok_or("use --patient <name|DID>")?;
            let patient = app.find_patient(&query)?;
            Ok(app.documents(Some(&patient.did))?)
        }
        Role::Patient => Ok(app.documents(None)?),
    }
}

fn print_documents(app: &EhrApp, list: &[IndexEntry]) {
    if list.is_empty() {
        println!("[DOC] LIST empty");
    }
    for (number, entry) in list.iter().enumerate() {
        println!(
            "[DOC] {} \"{}\" {} {} {} by=\"{}\" version={} valid_until={}",
            number + 1,
            entry.file_name,
            date_text(entry.created_utc),
            entry.category.label(),
            size_text(entry.size),
            app.display_name(&entry.issuer_did),
            entry.version,
            date_text(entry.valid_until_utc)
        );
    }
}

pub(super) async fn execute(
    app: &Arc<EhrApp>,
    node: Option<&EhrNode>,
    words: Vec<String>,
) -> CliResult<()> {
    let mut words = words;
    let head: Vec<&str> = words.iter().take(2).map(String::as_str).collect();
    match head.as_slice() {
        ["identity", "show"] => print_profile(app),
        ["invite"] => {
            let address = EhrApp::node_address(&NetworkConfig::listen_from_env()?)?;
            println!("[INVITE] {}", app.create_invite(&address)?);
        }
        ["contacts"] => {
            let kinds: &[ContactKind] = match app.role() {
                Role::Doctor => &[ContactKind::Patient, ContactKind::Pending],
                Role::Patient => &[ContactKind::Doctor],
            };
            for kind in kinds {
                for contact in app.contacts(*kind)? {
                    println!(
                        "[CONTACT] {kind:?} \"{}\" did={}",
                        contact.person.display_name(),
                        contact.did
                    );
                }
            }
        }
        ["contact", "accept"] => {
            let name = take_option(&mut words, "--name")?;
            let did = words
                .get(2)
                .ok_or("use: contact accept <DID> [--name \"<name>\"]")?;
            app.accept_contact(did, name.as_deref())?;
        }
        ["contact", "reject"] => {
            app.reject_contact(words.get(2).ok_or("use: contact reject <DID>")?)?;
            println!("[CONTACT] REJECTED");
        }
        ["contact", "show"] => {
            let query = words.get(2).ok_or("use: contact show <name|DID>")?;
            let kind = match app.role() {
                Role::Doctor => ContactKind::Patient,
                Role::Patient => ContactKind::Doctor,
            };
            let query = query.to_lowercase();
            let contact = app
                .contacts(kind)?
                .into_iter()
                .find(|contact| {
                    contact.did == query
                        || contact
                            .person
                            .display_name()
                            .to_lowercase()
                            .contains(&query)
                })
                .ok_or("contact not found")?;
            println!("{}", serde_json::to_string_pretty(&contact.person)?);
        }
        ["connect", ..] => {
            let invitation = words.get(1).ok_or("use: connect <invitation ehr1:...>")?;
            require_node(node)?.connect_doctor(invitation).await?;
        }
        ["inbox", "sync"] => {
            require_node(node)?.sync_inbox().await?;
        }
        ["doc", "list"] => {
            let list = documents(app, &mut words)?;
            print_documents(app, &list);
        }
        ["doc", "show"] => {
            let list = documents(app, &mut words)?;
            let entry =
                EhrApp::resolve_document(&list, words.get(2).ok_or("use: doc show <#|name>")?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&DocumentReference::from_entry(&entry)?)?
            );
        }
        ["doc", "issue"] => {
            let query = take_option(&mut words, "--patient")?.ok_or("use --patient <name|DID>")?;
            let name = take_option(&mut words, "--name")?;
            let category = match take_option(&mut words, "--category")? {
                Some(code) => {
                    Category::from_code(&code).ok_or("category: lab|imaging|discharge|other")?
                }
                None => Category::Other,
            };
            let path = absolute(
                words
                    .get(2)
                    .ok_or("use: doc issue <absolute-file> --patient <name>")?,
            )?;
            let patient = app.find_patient(&query)?;
            require_node(node)?
                .issue_document(NewDocument {
                    path,
                    patient_did: patient.did,
                    file_name: name.unwrap_or_default(),
                    category,
                })
                .await?;
        }
        ["doc", "update"] => {
            let list = documents(app, &mut words)?;
            let entry = EhrApp::resolve_document(
                &list,
                words.get(2).ok_or("use: doc update <#|name> <file>")?,
            )?;
            let path = absolute(
                words
                    .get(3)
                    .ok_or("use: doc update <#|name> <absolute-file>")?,
            )?;
            require_node(node)?.update_document(&entry, &path).await?;
        }
        ["doc", "open"] => {
            let list = documents(app, &mut words)?;
            let entry = EhrApp::resolve_document(
                &list,
                words.get(2).ok_or("use: doc open <#|name> <output>")?,
            )?;
            let output = absolute(
                words
                    .get(3)
                    .ok_or("use: doc open <#|name> <absolute-output>")?,
            )?;
            require_node(node)?
                .save_document(&entry, Path::new(&output))
                .await?;
        }
        _ => {
            return Err(format!(
                "unknown EHR command; see {} --help",
                binary_name(app.role())
            )
            .into());
        }
    }
    Ok(())
}

//! Komendy EHR (warstwa kontrolera): mapują argumenty na metody `ehr-core` i wypisują wynik.
//! Te same komendy działają offline (`ehr-cli-<rola> doc list`) i w stdin uruchomionego węzła;
//! komendy sieciowe (`doc issue`, `connect`, `inbox sync`, `doc open`) tylko w węźle.
use crate::cli::{CliResult, binary_name, ipfs_line};
use crate::services::NetworkService;
use ehr_core::utils::{date_text, media_type_for, size_text};
use ehr_core::{
    Category, ContactKind, DocumentReference, EhrApp, EhrNode, IndexEntry, NewDocument, Role,
};
use ipfs_network::{DhtMode, NetworkConfig};
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PASSWORD_ENV: &str = "EHR_PASSWORD";

pub(crate) fn usage(role: Role) -> String {
    let name = binary_name(role);
    let common = format!(
        "EHR profile (password from {PASSWORD_ENV} or prompt):\n\
  {name} identity init --name \"<first and last name>\"\n\
  {name} identity show\n\
  {name} contacts\n\
  {name} contact show <name|DID>          (HL7 FHIR R5 Patient/Practitioner)\n"
    );
    match role {
        Role::Doctor => format!(
            "{common}\
  {name} invite                           (one-time invitation for a patient)\n\
  {name} contact accept <DID> [--name \"<name>\"] | contact reject <DID>\n\
  {name} doc list --patient <name|DID>\n\
  {name} doc show <#|file name> --patient <name|DID>   (HL7 FHIR R5 DocumentReference)\n\
During node start also:\n\
  doc issue <absolute-file> --patient <name|DID> [--name \"<file name>\"] [--category lab|imaging|discharge|other]\n\
  doc update <#|file name> <absolute-file> --patient <name|DID>\n\
  doc open <#|file name> <absolute-output> --patient <name|DID>\n\
Documents are signed and encrypted (OpenPGP) for the patient and the doctor, imported as\n\
UnixFS and announced; the patient's read permission (UCAN) is valid for 20 years."
        ),
        Role::Patient => format!(
            "{common}\
  {name} doc list\n\
  {name} doc show <#|file name>\n\
During node start also:\n\
  connect <invitation ehr1:...>           (sends your card to the doctor)\n\
  inbox sync                              (also automatic every 30 s)\n\
  doc open <#|file name> <absolute-output>\n\
The patient only reads and downloads documents issued by doctors from the contacts."
        ),
    }
}

/// PeerId bez hasła: z DID profilu albo z `./tmp/node-key.pb`, gdy profilu nie ma.
pub(crate) fn local_peer_id() -> CliResult<String> {
    let data_dir = EhrApp::default_data_dir();
    if let Ok(bytes) = std::fs::read(data_dir.join(ehr_core::consts::PROFILE_FILE))
        && EhrApp::profile_exists(&data_dir)
    {
        let profile: ehr_core::Profile = serde_json::from_slice(&bytes)?;
        return Ok(ehr_core::utils::peer_id_from_did(&profile.did)?.to_string());
    }
    NetworkService::local_peer_id()
}

pub(crate) fn is_offline_command(command: &str) -> bool {
    matches!(
        command,
        "identity" | "invite" | "contacts" | "contact" | "doc"
    )
}

fn is_node_command(command: &str) -> bool {
    is_offline_command(command) || matches!(command, "connect" | "inbox")
}

fn password(confirm: bool) -> CliResult<String> {
    if let Ok(password) = std::env::var(PASSWORD_ENV) {
        return Ok(password);
    }
    let password = rpassword::prompt_password("Hasło: ")?;
    if confirm && rpassword::prompt_password("Powtórz hasło: ")? != password {
        return Err("passwords differ".into());
    }
    Ok(password)
}

fn open_app(role: Role) -> CliResult<Arc<EhrApp>> {
    Ok(Arc::new(EhrApp::open(
        &EhrApp::default_data_dir(),
        role,
        &password(false)?,
    )?))
}

/// Dzieli linię na słowa; tekst w cudzysłowie jest jednym słowem (`--name "Jan Kowalski"`).
fn split_args(line: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted, mut started) = (Vec::new(), String::new(), false, false);
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// Usuwa `--flag <wartość>` z listy i zwraca wartość.
fn take_option(words: &mut Vec<String>, flag: &str) -> CliResult<Option<String>> {
    let Some(index) = words.iter().position(|word| word == flag) else {
        return Ok(None);
    };
    if index + 1 >= words.len() {
        return Err(format!("{flag} needs a value").into());
    }
    let value = words.remove(index + 1);
    words.remove(index);
    Ok(Some(value))
}

pub(crate) async fn offline(role: Role, args: &[String]) -> CliResult<()> {
    let words: Vec<String> = args.to_vec();
    if words.first().map(String::as_str) == Some("identity")
        && words.get(1).map(String::as_str) == Some("init")
    {
        let mut words = words;
        let name =
            take_option(&mut words, "--name")?.ok_or("use: identity init --name \"<name>\"")?;
        let data_dir = EhrApp::default_data_dir();
        std::fs::create_dir_all(&data_dir)?;
        let app = EhrApp::create_profile(&data_dir, role, &name, &password(true)?)?;
        print_profile(&app);
        return Ok(());
    }
    execute(&open_app(role)?, None, words).await
}

fn print_profile(app: &EhrApp) {
    let profile = app.profile();
    println!(
        "[PROFILE] name=\"{}\" role={} did={} peer={}",
        profile.name,
        profile.role.label(),
        profile.did,
        app.identity().peer_id()
    );
}

/// `node start` z profilem: klucz odblokowany hasłem, protokoły EHR i komendy w stdin.
pub(crate) async fn run_node(role: Role, config: NetworkConfig) -> CliResult<()> {
    let app = open_app(role)?;
    let (node, handle) = app
        .clone()
        .start_node(
            config.listen.clone(),
            config.bootstrap,
            config.dht_mode == DhtMode::Server,
        )
        .await?;
    print_profile(&app);
    println!(
        "[DHT] LAN {:?}",
        if role == Role::Doctor {
            DhtMode::Server
        } else {
            config.dht_mode
        }
    );
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let words = split_args(&line);
            match words.first() {
                Some(command) if is_node_command(command) => {
                    if let Err(error) = runtime.block_on(execute(&app, Some(&node), words)) {
                        eprintln!("[EHR_FAILED] {error}");
                    }
                }
                _ => {
                    if !ipfs_line(&line, node.commands()) {
                        break;
                    }
                }
            }
        }
    });
    handle.await??;
    Ok(())
}

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

/// Dokumenty z listy, którą widzi użytkownik: lekarz u wskazanego pacjenta, pacjent własne.
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

async fn execute(app: &Arc<EhrApp>, node: Option<&EhrNode>, words: Vec<String>) -> CliResult<()> {
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
                .find(|c| c.did == query || c.person.display_name().to_lowercase().contains(&query))
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
            let file_name = name.unwrap_or_else(|| {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            require_node(node)?
                .issue_document(NewDocument {
                    media_type: media_type_for(&file_name).into(),
                    path,
                    patient_did: patient.did,
                    file_name,
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

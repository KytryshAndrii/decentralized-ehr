//! EHR commands (controller layer): map arguments to `ehr-core` methods and print the result.
//! The same commands work offline (`ehr-cli-<role> doc list`) and on the stdin of a running node;
//! network commands (`doc issue`, `connect`, `inbox sync`, `doc open`) only in the node.
mod execute;

use crate::cli::{CliResult, binary_name, ipfs_line};
use crate::services::NetworkService;
use ehr_core::{EhrApp, Role};
use execute::execute;
use ipfs_network::{DhtMode, NetworkConfig};
use std::io::{self, BufRead};
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

/// PeerId without the password: from the profile DID, or from `./tmp/node-key.pb` without a profile.
pub(crate) fn local_peer_id(role: Role) -> CliResult<String> {
    let data_dir = EhrApp::default_data_dir(role);
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
        &EhrApp::default_data_dir(role),
        role,
        &password(false)?,
    )?))
}

/// Splits a line into words; quoted text is one word (`--name "Jan Kowalski"`).
fn split_args(line: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted, mut started) = (Vec::new(), String::new(), false, false);
    for character in line.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            character if character.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            character => {
                word.push(character);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// Removes `--flag <value>` from the list and returns the value.
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
        let data_dir = EhrApp::default_data_dir(role);
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

/// `node start` with a profile: key unlocked by the password, EHR protocols and stdin commands.
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
        if node.dht_server() {
            DhtMode::Server
        } else {
            DhtMode::Client
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
                    if !ipfs_line(app.data_dir(), &line, node.commands()) {
                        break;
                    }
                }
            }
        }
    });
    handle.await??;
    Ok(())
}

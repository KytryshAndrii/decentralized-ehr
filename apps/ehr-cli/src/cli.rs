use crate::ehr_commands;
use crate::services::{IpfsService, NetworkCommand, NetworkService};
use ehr_core::{EhrApp, Role};
use ipfs_store::consts::DEFAULT_DATA_DIR;
use std::{
    env,
    error::Error,
    io::{self, BufRead},
    path::{Path, PathBuf},
};

pub type CliResult<T> = Result<T, Box<dyn Error>>;
const HELP_HINT: &str = "EHR CLI: use --help flag to see available commands";

fn usage(role: Role) {
    let name = binary_name(role);
    println!(
        "EHR CLI ({})\n\n\
Usage:\n\
  {name} --help\n\
  {name} node identity\n\
  {name} node start [--dht-server]\n\
  {name} node start \"\" [--dht-server]\n\
  {name} node start --bootstrap <multiaddr/p2p/PeerId> [--dht-server]\n\
  {name} node start --bootstrap-list <addr1>,<addr2>,... [--dht-server]\n\
  {name} cid <absolute-file-path>\n\
  {name} put <absolute-file-path>\n\
  {name} add <absolute-file-path>\n\
  {name} has <CID>\n\
  {name} get <CID> <absolute-output-path>\n\n\
put stores a file as one raw block (up to 2 MiB). add imports a file of any size as UnixFS\n\
(balanced DAG, raw leaves, fixed 256 KiB chunks).\n\
get writes a raw block or a whole UnixFS file.\n\
Identity: ./tmp/node-key.pb (auto-created or reused); with an EHR profile the node key is\n\
./ehr-<doctor|patient>-data/keys/node-key.enc, protected by the profile password.\n\
Listen: /ip4/0.0.0.0/tcp/4101 (port override: EHR_LISTEN_PORT).\n\
Bootstrap: default peer when omitted, none with \"\", several with --bootstrap-list.\n\
State: known peers and provided CIDs are saved in ./data and restored on start.\n\
DHT: LAN only; Client by default, Server with --dht-server.\n\
Bitswap: provided blocks are served to connected peers over /ipfs/bitswap/1.2.0.\n\
provide and stop cover every block of a UnixFS file.\n\
Fetch: fetch <CID> finds providers in the DHT, downloads and verifies the block or the whole\n\
UnixFS file, stores it locally.\n\
During node start: peers, bootstrap, provide <CID>, find <CID>, fetch <CID>, stop <CID>, quit.\n\n\
Custom bootstrap address format: /ip4/<IPv4>/tcp/<port>/p2p/<PeerId>\n\
  e.g. /ip4/192.168.1.125/tcp/4101/p2p/12D3KooW...\n\
  (the PeerId of a node is printed by: {name} node identity)\n\n\
{}",
        role.label(),
        ehr_commands::usage(role)
    );
}

pub(crate) fn binary_name(role: Role) -> &'static str {
    match role {
        Role::Doctor => "ehr-cli-doctor",
        Role::Patient => "ehr-cli-patient",
    }
}

fn argument_at<'a>(args: &'a [String], index: usize, description: &str) -> CliResult<&'a str> {
    Ok(args
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("Missing argument: {description}"))?)
}

fn exact_args(args: &[String], expected: usize) -> CliResult<()> {
    if args.len() != expected {
        return Err(format!(
            "Expected {} arguments, got {}",
            expected - 1,
            args.len() - 1
        )
        .into());
    }
    Ok(())
}

/// Blockstore of this binary: the role profile folder, or `./data` without a profile, like a plain IPFS node.
pub(crate) fn store_dir(role: Role) -> PathBuf {
    let profile_dir = EhrApp::default_data_dir(role);
    if EhrApp::profile_exists(&profile_dir) {
        profile_dir
    } else {
        PathBuf::from(DEFAULT_DATA_DIR)
    }
}

fn local_dag(
    store: &Path,
    cid: &ipfs_cid::models::Cid,
) -> crate::services::ipfs_service::ServiceResult<Vec<ipfs_cid::models::Cid>> {
    IpfsService::new(store)?.dag_cids(cid)
}

/// IPFS commands in the running node; `false` after `quit` or when the node stopped.
pub(crate) fn ipfs_line(
    store: &Path,
    line: &str,
    sender: &tokio::sync::mpsc::UnboundedSender<NetworkCommand>,
) -> bool {
    let mut parts = line.split_whitespace();
    let Some(command) = parts.next() else {
        return true;
    };
    let argument = parts.next();
    let parsed = match (command, argument, parts.next()) {
        ("peers", None, None) => Ok(NetworkCommand::ListPeers),
        ("bootstrap", None, None) => Ok(NetworkCommand::Bootstrap),
        ("quit", None, None) => Ok(NetworkCommand::Quit),
        ("provide", Some(cid), None) => IpfsService::parse_cid(cid)
            .map(NetworkCommand::StartProviding)
            .map_err(|error| error.to_string()),
        ("find", Some(cid), None) => IpfsService::parse_cid(cid)
            .map(NetworkCommand::GetProviders)
            .map_err(|error| error.to_string()),
        ("fetch", Some(cid), None) => IpfsService::parse_cid(cid)
            .map(NetworkCommand::Fetch)
            .map_err(|error| error.to_string()),
        ("stop", Some(cid), None) => IpfsService::parse_cid(cid)
            .map(NetworkCommand::StopProviding)
            .map_err(|error| error.to_string()),
        _ => Err(
            "Use peers, bootstrap, provide <CID>, find <CID>, fetch <CID>, stop <CID> or quit"
                .into(),
        ),
    };
    match parsed {
        // Other nodes fetch the file block by block, so the whole DAG is announced.
        // The root goes last: once it can be found, every block is already served.
        Ok(NetworkCommand::StartProviding(cid)) => match local_dag(store, &cid) {
            Ok(cids) => {
                for cid in cids.into_iter().rev() {
                    if sender.send(NetworkCommand::StartProviding(cid)).is_err() {
                        return false;
                    }
                }
            }
            Err(error) => eprintln!("DHT_PROVIDE_FAILED {error}"),
        },
        // When the DAG is not complete locally, only the given block is withdrawn.
        Ok(NetworkCommand::StopProviding(cid)) => {
            for cid in local_dag(store, &cid).unwrap_or_else(|_| vec![cid]) {
                if sender.send(NetworkCommand::StopProviding(cid)).is_err() {
                    return false;
                }
            }
        }
        Ok(NetworkCommand::Fetch(cid)) if local_dag(store, &cid).is_ok() => {
            println!("BITSWAP_ALREADY_LOCAL {}", cid.to_string_base32());
        }
        Ok(command) => {
            let quit = matches!(command, NetworkCommand::Quit);
            if sender.send(command).is_err() || quit {
                return false;
            }
        }
        Err(error) => eprintln!("DHT_COMMAND_ERROR {error}"),
    }
    true
}

fn stdin_commands(store: PathBuf, sender: tokio::sync::mpsc::UnboundedSender<NetworkCommand>) {
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if !ipfs_line(&store, &line, &sender) {
            break;
        }
    }
}

async fn node_command(role: Role, args: &[String]) -> CliResult<()> {
    match argument_at(args, 2, "node subcommand")? {
        "identity" => {
            exact_args(args, 3)?;
            println!("LOCAL_PEER_ID {}", ehr_commands::local_peer_id(role)?);
        }
        "start" => {
            let server_flag_count = args[3..]
                .iter()
                .filter(|argument| *argument == "--dht-server")
                .count();
            if server_flag_count > 1 {
                return Err("--dht-server may be specified once".into());
            }
            let remaining: Vec<String> = args[3..]
                .iter()
                .filter(|argument| argument.as_str() != "--dht-server")
                .cloned()
                .collect();
            let config = NetworkService::node_config(server_flag_count == 1, &remaining)?;
            println!("[NETWORK] LISTEN {}", config.listen);
            for peer in &config.bootstrap {
                println!("[NETWORK] BOOTSTRAP {}/p2p/{}", peer.address, peer.peer_id);
            }
            if config.bootstrap.is_empty() {
                println!("[NETWORK] LISTEN ONLY (no bootstrap)");
            }
            // With an EHR profile: key unlocked by the password and doctor or patient commands.
            if EhrApp::profile_exists(&EhrApp::default_data_dir(role)) {
                return ehr_commands::run_node(role, config).await;
            }
            println!("[NETWORK] IDENTITY_PATH {}", config.identity_path.display());
            println!("[DHT] LAN {:?}", config.dht_mode);
            let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
            std::thread::spawn(move || stdin_commands(store_dir(role), sender));
            NetworkService::run(config, receiver).await?;
        }
        _ => return Err("Unknown node command".into()),
    }
    Ok(())
}
fn storage_command(role: Role, args: &[String]) -> CliResult<()> {
    let store = store_dir(role);
    match argument_at(args, 1, "storage command")? {
        "cid" => {
            exact_args(args, 3)?;
            let path = Path::new(argument_at(args, 2, "absolute file path")?);
            let cid = IpfsService::create_cid(path)?;
            println!(
                "[CID] CREATED file={} codec=raw hash=sha2-256 CID={}",
                path.display(),
                cid.to_string_base32()
            );
        }
        "put" => {
            exact_args(args, 3)?;
            let path = Path::new(argument_at(args, 2, "absolute file path")?);
            let service = IpfsService::new(&store)?;
            let cid = service.put_file(path)?;
            println!("[BLOCKSTORE] PUT CID={}", cid.to_string_base32());
            println!("[BLOCKSTORE] HAS {}", service.has_block(&cid)?);
        }
        "add" => {
            exact_args(args, 3)?;
            let path = Path::new(argument_at(args, 2, "absolute file path")?);
            let (file, chunker) = IpfsService::new(&store)?.add_file(path)?;
            println!(
                "[UNIXFS] ADDED CID={} bytes={} chunker={} new_blocks={} reused_blocks={}",
                file.root.to_string_base32(),
                file.size,
                chunker.kubo_name(),
                file.new_blocks,
                file.reused_blocks
            );
        }
        "has" => {
            exact_args(args, 3)?;
            let cid = IpfsService::parse_cid(argument_at(args, 2, "CID")?)?;
            let service = IpfsService::new(&store)?;
            println!("[BLOCKSTORE] HAS {}", service.has_block(&cid)?);
        }
        "get" => {
            exact_args(args, 4)?;
            let cid = IpfsService::parse_cid(argument_at(args, 2, "CID")?)?;
            let output = Path::new(argument_at(args, 3, "absolute output path")?);
            let service = IpfsService::new(&store)?;
            if !service.export_file(&cid, output)? {
                return Err(format!("Block not found: {}", cid.to_string_base32()).into());
            }
            println!("[BLOCKSTORE] GET saved={}", output.display());
        }
        _ => return Err("Unknown storage command".into()),
    }
    Ok(())
}

/// Entry point of both binaries; `role` is fixed per binary.
pub fn run(role: Role) -> CliResult<()> {
    std::fs::create_dir_all(EhrApp::default_data_dir(role))?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let args: Vec<String> = env::args().collect();
        match args.get(1).map(String::as_str) {
            Some("node") => node_command(role, &args).await,
            Some("cid" | "put" | "add" | "has" | "get") => storage_command(role, &args),
            Some("--help" | "-h" | "help") if args.len() == 2 => {
                usage(role);
                Ok(())
            }
            Some(command) if ehr_commands::is_offline_command(command) => {
                ehr_commands::offline(role, &args[1..]).await
            }
            None => {
                println!("{HELP_HINT}");
                Ok(())
            }
            _ => {
                eprintln!("{HELP_HINT}");
                Err("invalid arguments".into())
            }
        }
    })
}

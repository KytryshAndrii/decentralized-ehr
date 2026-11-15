mod services;

use ipfs_cid::models::Cid;
use ipfs_network::{BootstrapPeer, DhtMode, NetworkCommand, NetworkConfig, NetworkNode};
use services::IpfsService;
use std::{
    env,
    error::Error,
    io::{self, BufRead},
    path::Path,
};

type CliResult<T> = Result<T, Box<dyn Error>>;
const HELP_HINT: &str = "EHR CLI: use --help flag to see available commands";

fn usage() {
    println!(
        "EHR CLI\n\n\
Usage:\n\
  ehr-cli --help\n\
  ehr-cli node identity\n\
  ehr-cli node start [--dht-server]\n\
  ehr-cli node start \"\" [--dht-server]\n\
  ehr-cli node start --bootstrap <multiaddr/p2p/PeerId> [--dht-server]\n\
  ehr-cli node start --bootstrap-list <addr1>,<addr2>,... [--dht-server]\n\
  ehr-cli cid <absolute-file-path>\n\
  ehr-cli put <absolute-file-path>\n\
  ehr-cli has <CID>\n\
  ehr-cli get <CID> <absolute-output-path>\n\n\
Identity: ./tmp/node-key.pb (auto-created or reused).\n\
Listen: /ip4/0.0.0.0/tcp/4101 (port override: EHR_LISTEN_PORT).\n\
Bootstrap: default peer when omitted, none with \"\", several with --bootstrap-list.\n\
State: known peers and provided CIDs are saved in ./data and restored on start.\n\
DHT: LAN only; Client by default, Server with --dht-server.\n\
Bitswap: provided blocks are served to connected peers over /ipfs/bitswap/1.2.0.\n\
During node start: peers, bootstrap, provide <CID>, find <CID>, stop <CID>, quit.\n\n\
Custom bootstrap address format: /ip4/<IPv4>/tcp/<port>/p2p/<PeerId>\n\
  e.g. /ip4/192.168.1.125/tcp/4101/p2p/12D3KooW...\n\
  (the PeerId of a node is printed by: ehr-cli node identity)"
    );
}
fn arg<'a>(args: &'a [String], index: usize, description: &str) -> CliResult<&'a str> {
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
/// Listen address is hardcoded; arguments only choose the bootstrap peer:
/// none -> default bootstrap, "" -> no bootstrap, --bootstrap <addr> -> one custom peer,
/// --bootstrap-list <a>,<b> -> several custom peers.
fn configure_node_start(config: &mut NetworkConfig, extra: &[String]) -> CliResult<()> {
    match extra {
        [] => config.bootstrap.push(BootstrapPeer::default_bootstrap()?),
        [address] if address.is_empty() => {}
        [flag, address] if flag == "--bootstrap" => {
            config.bootstrap.push(BootstrapPeer::parse(address)?);
        }
        [flag, list] if flag == "--bootstrap-list" => {
            config.bootstrap.extend(BootstrapPeer::parse_list(list)?);
        }
        _ => {
            return Err(
                "Use: node start [\"\" | --bootstrap <multiaddr/p2p/PeerId> | --bootstrap-list <addr1>,<addr2>,...] [--dht-server]".into(),
            );
        }
    }
    Ok(())
}

fn stdin_commands(sender: tokio::sync::mpsc::UnboundedSender<NetworkCommand>) {
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let mut parts = line.split_whitespace();
        let Some(command) = parts.next() else {
            continue;
        };
        let argument = parts.next();
        let parsed = match (command, argument, parts.next()) {
            ("peers", None, None) => Ok(NetworkCommand::ListPeers),
            ("bootstrap", None, None) => Ok(NetworkCommand::Bootstrap),
            ("quit", None, None) => Ok(NetworkCommand::Quit),
            ("provide", Some(cid), None) => Cid::decode_from_string(cid)
                .map(NetworkCommand::StartProviding)
                .map_err(|error| error.to_string()),
            ("find", Some(cid), None) => Cid::decode_from_string(cid)
                .map(NetworkCommand::GetProviders)
                .map_err(|error| error.to_string()),
            ("stop", Some(cid), None) => Cid::decode_from_string(cid)
                .map(NetworkCommand::StopProviding)
                .map_err(|error| error.to_string()),
            _ => Err("Use peers, bootstrap, provide <CID>, find <CID>, stop <CID> or quit".into()),
        };
        match parsed {
            Ok(NetworkCommand::StartProviding(cid)) => {
                match IpfsService::new().and_then(|service| service.has_block(&cid)) {
                    Ok(true) => {
                        if sender.send(NetworkCommand::StartProviding(cid)).is_err() {
                            break;
                        }
                    }
                    Ok(false) => eprintln!("DHT_PROVIDE_FAILED block is not in local Blockstore"),
                    Err(error) => eprintln!("DHT_PROVIDE_FAILED {error}"),
                }
            }
            Ok(command) => {
                let quit = matches!(command, NetworkCommand::Quit);
                if sender.send(command).is_err() || quit {
                    break;
                }
            }
            Err(error) => eprintln!("DHT_COMMAND_ERROR {error}"),
        }
    }
}

async fn node_command(args: &[String]) -> CliResult<()> {
    match arg(args, 2, "node subcommand")? {
        "identity" => {
            exact_args(args, 3)?;
            let key_path = NetworkConfig::default().identity_path;
            let key = ipfs_network::identity::load_or_create(&key_path)?;
            println!("LOCAL_PEER_ID {}", ipfs_network::peer_id_from_keypair(&key));
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
            let mut config = NetworkConfig {
                listen: NetworkConfig::listen_from_env()?,
                state_directory: Some(ipfs_store::consts::DEFAULT_DATA_DIR.into()),
                ..NetworkConfig::default()
            };
            if server_flag_count == 1 {
                config.dht_mode = DhtMode::Server;
            }
            configure_node_start(&mut config, &remaining)?;
            println!("[NETWORK] IDENTITY_PATH {}", config.identity_path.display());
            println!("[NETWORK] LISTEN {}", config.listen);
            for peer in &config.bootstrap {
                println!("[NETWORK] BOOTSTRAP {}/p2p/{}", peer.address, peer.peer_id);
            }
            if config.bootstrap.is_empty() {
                println!("[NETWORK] LISTEN ONLY (no bootstrap)");
            }
            println!("[DHT] LAN {:?}", config.dht_mode);
            let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
            std::thread::spawn(move || stdin_commands(sender));
            NetworkNode::new_network_node(config)?
                .run_with_commands(receiver)
                .await?;
        }
        _ => return Err("Unknown node command".into()),
    }
    Ok(())
}
fn storage_command(args: &[String]) -> CliResult<()> {
    match arg(args, 1, "storage command")? {
        "cid" => {
            exact_args(args, 3)?;
            let path = Path::new(arg(args, 2, "absolute file path")?);
            let cid = IpfsService::create_cid(path)?;
            println!(
                "[CID] CREATED file={} codec=raw hash=sha2-256 CID={}",
                path.display(),
                cid.to_string_base32()
            );
        }
        "put" => {
            exact_args(args, 3)?;
            let path = Path::new(arg(args, 2, "absolute file path")?);
            let service = IpfsService::new()?;
            let cid = service.put_file(path)?;
            println!("[BLOCKSTORE] PUT CID={}", cid.to_string_base32());
            println!("[BLOCKSTORE] HAS {}", service.has_block(&cid)?);
        }
        "has" => {
            exact_args(args, 3)?;
            let cid = Cid::decode_from_string(arg(args, 2, "CID")?)?;
            let service = IpfsService::new()?;
            println!("[BLOCKSTORE] HAS {}", service.has_block(&cid)?);
        }
        "get" => {
            exact_args(args, 4)?;
            let cid = Cid::decode_from_string(arg(args, 2, "CID")?)?;
            let output = Path::new(arg(args, 3, "absolute output path")?);
            let service = IpfsService::new()?;
            if !service.export_file(&cid, output)? {
                return Err(format!("Block not found: {}", cid.to_string_base32()).into());
            }
            println!("[BLOCKSTORE] GET saved={}", output.display());
        }
        _ => return Err("Unknown storage command".into()),
    }
    Ok(())
}
#[tokio::main]
async fn main() -> CliResult<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("node") => node_command(&args).await,
        Some("cid" | "put" | "has" | "get") => storage_command(&args),
        Some("--help" | "-h" | "help") if args.len() == 2 => {
            usage();
            Ok(())
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
}

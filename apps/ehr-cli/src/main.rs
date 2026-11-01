mod services;

use ipfs_cid::models::Cid;
use ipfs_network::{BootstrapPeer, NetworkConfig, NetworkNode};
use services::IpfsService;
use std::{env, error::Error, path::Path};

type CliResult<T> = Result<T, Box<dyn Error>>;

fn usage() {
    eprintln!(
        "EHR CLI\n\n\
Usage:\n\
  ehr-cli help\n\
  ehr-cli node identity\n\
  ehr-cli node start <listen-multiaddr>\n\
  ehr-cli node start <listen-multiaddr> --default-bootstrap\n\
  ehr-cli node start <listen-multiaddr> --bootstrap <multiaddr/p2p/PeerId>\n\
  ehr-cli cid <absolute-file-path>\n\
  ehr-cli put <absolute-file-path>\n\
  ehr-cli has <CID>\n\
  ehr-cli get <CID> <absolute-output-path>\n\n\
Identity: ./tmp/node-key.pb (auto-created or reused).\n\
Note: --default-bootstrap and --bootstrap cannot be combined.\n"
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

/// Exactly one of: listen only, default bootstrap, or custom bootstrap.
/// The legacy positional multiaddr remains an alias for custom bootstrap,
/// preserving existing integration tests without adding a fourth mode.
fn configure_bootstrap(config: &mut NetworkConfig, extra: &[String]) -> CliResult<()> {
    match extra {
        [] => Ok(()),
        [flag] if flag == "--default-bootstrap" => {
            config.bootstrap.push(BootstrapPeer::default_bootstrap()?);
            Ok(())
        }
        [flag, address] if flag == "--bootstrap" => {
            config.bootstrap.push(BootstrapPeer::parse(address)?);
            Ok(())
        }
        [address] if address.starts_with('/') => {
            config.bootstrap.push(BootstrapPeer::parse(address)?);
            Ok(())
        }
        _ => Err(
            "Choose only one mode: listen only, --default-bootstrap, or --bootstrap <address>"
                .into(),
        ),
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
            if args.len() < 4 {
                return Err("node start requires listen address".into());
            }
            let mut config = NetworkConfig {
                listen: args[3].parse()?,
                ..NetworkConfig::default()
            };
            configure_bootstrap(&mut config, &args[4..])?;
            println!("[NETWORK] IDENTITY_PATH {}", config.identity_path.display());
            println!("[NETWORK] LISTEN {}", config.listen);
            if let Some(peer) = config.bootstrap.first() {
                println!("[NETWORK] BOOTSTRAP {}/p2p/{}", peer.address, peer.peer_id);
            } else {
                println!("[NETWORK] LISTEN ONLY (no bootstrap)");
            }
            NetworkNode::new_network_node(config)?.run().await?;
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
            usage();
            Ok(())
        }
        _ => {
            usage();
            Err("invalid arguments".into())
        }
    }
}

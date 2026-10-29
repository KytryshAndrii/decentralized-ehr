use ipfs_network::{BootstrapPeer, NetworkConfig, NetworkNode};
use std::{env, error::Error, path::PathBuf};

fn usage() {
    eprintln!(
        "EHR CLI (phase 4)\n\nUsage:\n  ehr-cli node identity <identity-file>\n  ehr-cli node start <identity-file> <listen-multiaddr> [bootstrap-multiaddr/p2p/<PeerId> ...]\n\nExamples:\n  ehr-cli node identity ./tmp/a.key.pb\n  ehr-cli node start ./tmp/a.key.pb /ip4/127.0.0.1/tcp/4101\n\nNote: this is the first iteration of the console application; interactive TUI comes later."
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("node") => match args.get(2).map(String::as_str) {
            Some("identity") if args.len() == 4 => {
                let key =
                    ipfs_network::identity::load_or_create(PathBuf::from(&args[3]).as_path())?;
                println!("LOCAL_PEER_ID {}", ipfs_network::peer_id_from_keypair(&key));
                Ok(())
            }
            Some("start") if args.len() >= 5 => {
                let mut config = NetworkConfig {
                    identity_path: PathBuf::from(&args[3]),
                    listen: args[4].parse()?,
                    ..NetworkConfig::default()
                };

                for arg in args.iter().skip(5) {
                    config.bootstrap.push(BootstrapPeer::parse(arg)?);
                }
                NetworkNode::new_network_node(config)?.run().await?;
                Ok(())
            }
            _ => {
                usage();
                Err("invalid arguments".into())
            }
        },
        Some("--help") | Some("-h") | Some("help") => {
            usage();
            Ok(())
        }
        _ => {
            usage();
            Err("invalid arguments".into())
        }
    }
}

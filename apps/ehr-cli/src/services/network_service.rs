use super::ipfs_service::ServiceResult;
use ipfs_network::{BootstrapPeer, DhtMode, NetworkCommand, NetworkConfig, NetworkNode};
use ipfs_store::consts::DEFAULT_DATA_DIR;
use tokio::sync::mpsc::UnboundedReceiver;

const START_USAGE: &str = "Use: node start [\"\" | --bootstrap <multiaddr/p2p/PeerId> | --bootstrap-list <addr1>,<addr2>,...] [--dht-server]";

/// The network node as the CLI sees it: only the public `ipfs-network` API.
pub struct NetworkService;

impl NetworkService {
    /// PeerId from the key at the default path; the key is created on first use.
    pub fn local_peer_id() -> ServiceResult<String> {
        let key = ipfs_network::identity::load_or_create(&NetworkConfig::default().identity_path)?;
        Ok(ipfs_network::peer_id_from_keypair(&key).to_string())
    }

    /// The listen address is fixed; the arguments only pick the bootstrap:
    /// none -> default, "" -> no bootstrap, --bootstrap <addr> -> one, --bootstrap-list <a>,<b> -> several.
    pub fn node_config(dht_server: bool, bootstrap: &[String]) -> ServiceResult<NetworkConfig> {
        let mut config = NetworkConfig {
            listen: NetworkConfig::listen_from_env()?,
            state_directory: Some(DEFAULT_DATA_DIR.into()),
            ..NetworkConfig::default()
        };
        if dht_server {
            config.dht_mode = DhtMode::Server;
        }
        match bootstrap {
            [] => config.bootstrap.push(BootstrapPeer::default_bootstrap()?),
            [address] if address.is_empty() => {}
            [flag, address] if flag == "--bootstrap" => {
                config.bootstrap.push(BootstrapPeer::parse(address)?);
            }
            [flag, list] if flag == "--bootstrap-list" => {
                config.bootstrap.extend(BootstrapPeer::parse_list(list)?);
            }
            _ => return Err(START_USAGE.into()),
        }
        Ok(config)
    }

    pub async fn run(
        config: NetworkConfig,
        commands: UnboundedReceiver<NetworkCommand>,
    ) -> ServiceResult<()> {
        Ok(NetworkNode::new_network_node(config)?
            .run_with_commands(commands)
            .await?)
    }
}

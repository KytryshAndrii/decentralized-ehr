use super::ipfs_service::ServiceResult;
use ipfs_network::{BootstrapPeer, DhtMode, NetworkCommand, NetworkConfig, NetworkNode};
use ipfs_store::consts::DEFAULT_DATA_DIR;
use tokio::sync::mpsc::UnboundedReceiver;

const START_USAGE: &str = "Use: node start [\"\" | --bootstrap <multiaddr/p2p/PeerId> | --bootstrap-list <addr1>,<addr2>,...] [--dht-server]";

/// Węzeł sieciowy widziany przez CLI: tylko publiczne API `ipfs-network`.
pub struct NetworkService;

impl NetworkService {
    /// PeerId z klucza w domyślnej ścieżce; klucz powstaje przy pierwszym użyciu.
    pub fn local_peer_id() -> ServiceResult<String> {
        let key = ipfs_network::identity::load_or_create(&NetworkConfig::default().identity_path)?;
        Ok(ipfs_network::peer_id_from_keypair(&key).to_string())
    }

    /// Adres nasłuchu jest stały; argumenty wybierają tylko bootstrap:
    /// brak -> domyślny, "" -> żaden, --bootstrap <addr> -> jeden, --bootstrap-list <a>,<b> -> kilka.
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

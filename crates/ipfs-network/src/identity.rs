use crate::error::NetworkError;
use libp2p::identity::Keypair;
use std::io::Write;
use std::{fs, path::Path};

pub fn load_or_create(path: &Path) -> Result<Keypair, NetworkError> {
    if path.exists() {
        let data = fs::read(path)?;
        return Keypair::from_protobuf_encoding(&data)
            .map_err(|e| NetworkError::Identity(e.to_string()));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let identity = Keypair::generate_ed25519();
    let bytes = identity
        .to_protobuf_encoding()
        .map_err(|e| NetworkError::Identity(e.to_string()))?;

    // create_new prevents overwriting another identity. Do not silently rotate keys.
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let data = fs::read(path)?;
            return Keypair::from_protobuf_encoding(&data)
                .map_err(|e| NetworkError::Identity(e.to_string()));
        }
        Err(e) => return Err(e.into()),
    };
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(identity)
}

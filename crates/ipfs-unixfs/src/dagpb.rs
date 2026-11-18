use crate::error::UnixfsError;
use crate::models::{Child, DataType, PbLink, PbNode, UnixfsData};
use ipfs_cid::consts::{CODEC_DAG_PB, SHA2_256};
use ipfs_cid::models::{Cid, Multihash};
use prost::Message;

impl PbNode {
    /// Węzeł pliku UnixFS: same linki, dane są w liściach raw.
    pub(crate) fn file(children: &[Child]) -> Self {
        let unixfs = UnixfsData {
            r#type: DataType::File as i32,
            data: None,
            filesize: Some(children.iter().map(|child| child.file_bytes).sum()),
            blocksizes: children.iter().map(|child| child.file_bytes).collect(),
        };
        Self {
            data: Some(unixfs.encode_to_vec()),
            links: children
                .iter()
                .map(|child| PbLink {
                    hash: Some(child.cid.to_bytes()),
                    // Kubo zapisuje pustą nazwę jawnie; bez niej CID byłby inny.
                    name: Some(String::new()),
                    tsize: Some(child.dag_bytes),
                })
                .collect(),
        }
    }

    /// Kanoniczny dag-pb: linki (pole 2) przed danymi (pole 1). `prost` kodowałby
    /// w kolejności numerów pól, co dałoby inny CID niż Kubo.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for link in &self.links {
            prost::encoding::message::encode(2, link, &mut bytes);
        }
        if let Some(data) = &self.data {
            prost::encoding::bytes::encode(1, data, &mut bytes);
        }
        bytes
    }

    pub(crate) fn link_cids(&self) -> Result<Vec<Cid>, UnixfsError> {
        self.links
            .iter()
            .map(|link| link_cid(link.hash.as_deref().unwrap_or_default()))
            .collect()
    }
}

/// Link jako CIDv1 albo CIDv0 (goły multihash SHA2-256, zawsze dag-pb).
fn link_cid(bytes: &[u8]) -> Result<Cid, UnixfsError> {
    match bytes {
        [0x12, 0x20, digest @ ..] if digest.len() == 32 => Ok(Cid::new_cid(
            CODEC_DAG_PB,
            Multihash {
                code: SHA2_256,
                size: 32,
                digest: digest.to_vec(),
            },
        )),
        _ => Ok(Cid::decode_from_bytes(bytes)?),
    }
}

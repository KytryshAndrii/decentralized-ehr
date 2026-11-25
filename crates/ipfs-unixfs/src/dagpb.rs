use crate::error::UnixfsError;
use crate::models::{Child, DataType, PbLink, PbNode, UnixfsData};
use ipfs_cid::consts::{CODEC_DAG_PB, SHA2_256};
use ipfs_cid::models::{Cid, Multihash};
use prost::Message;

impl PbNode {
    /// UnixFS file node: only links, the data is in the raw leaves.
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
                    // Kubo writes the empty name explicitly; without it the CID would differ.
                    name: Some(String::new()),
                    tsize: Some(child.dag_bytes),
                })
                .collect(),
        }
    }

    /// Canonical dag-pb: links (field 2) before data (field 1). `prost` would encode in
    /// field number order, which gives a different CID than Kubo.
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

/// Link as CIDv1 or CIDv0 (bare SHA2-256 multihash, always dag-pb).
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

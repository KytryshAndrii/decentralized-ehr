use crate::consts::MAX_EXPORT_BYTES;
use crate::error::UnixfsError;
use crate::models::{DataType, PbNode, UnixfsData};
use ipfs_cid::consts::{CODEC_DAG_PB, CODEC_RAW};
use ipfs_cid::models::Cid;
use ipfs_store::{Blockstore, Datastore};
use prost::Message;
use std::collections::HashSet;
use std::io::Write;

/// CIDs the block links to. Raw blocks and unknown codecs have no links.
pub fn block_links(cid: &Cid, block: &[u8]) -> Result<Vec<Cid>, UnixfsError> {
    if cid.codec != CODEC_DAG_PB {
        return Ok(Vec::new());
    }
    PbNode::decode(block)?.link_cids()
}

/// One file block: its own data, its children and the sizes declared in UnixFS.
struct FileBlock {
    data: Vec<u8>,
    links: Vec<Cid>,
    /// Size of all content under this block (own data + children).
    declared: u64,
    blocksizes: Vec<u64>,
}

fn file_block(cid: &Cid, block: Vec<u8>) -> Result<FileBlock, UnixfsError> {
    let unsupported = |reason| UnixfsError::Unsupported {
        cid: cid.to_string_base32(),
        reason,
    };
    match cid.codec {
        CODEC_RAW => Ok(FileBlock {
            declared: block.len() as u64,
            data: block,
            links: Vec::new(),
            blocksizes: Vec::new(),
        }),
        CODEC_DAG_PB => {
            let node = PbNode::decode(block.as_slice())?;
            let data = node
                .data
                .as_deref()
                .ok_or(unsupported("dag-pb node without UnixFS data"))?;
            let unixfs = UnixfsData::decode(data)?;
            match unixfs.r#type() {
                DataType::File | DataType::Raw => {
                    let data = unixfs.data.unwrap_or_default();
                    Ok(FileBlock {
                        declared: unixfs.filesize.unwrap_or(data.len() as u64),
                        links: node.link_cids()?,
                        blocksizes: unixfs.blocksizes,
                        data,
                    })
                }
                _ => Err(unsupported("only UnixFS files are supported")),
            }
        }
        _ => Err(unsupported("codec is neither raw nor dag-pb")),
    }
}

/// Rebuilds the file from the DAG into `output` and returns the number of bytes.
/// Depth-first in link order, so trickle DAGs and Kubo dag-pb leaves work too.
/// Every block is checked against its CID by `Blockstore::get_block`.
///
/// Protection against a crafted DAG (e.g. 174 links to the same block on every level):
/// the root size is the limit (at most `MAX_EXPORT_BYTES`), every child must have a non-zero
/// size equal to the parent's `blocksizes`, and children plus own data must add up to the
/// declared node size. Going over stops the read before the file grows past the declaration.
pub fn cat<D: Datastore>(
    store: &Blockstore<D>,
    root: &Cid,
    output: &mut impl Write,
) -> Result<u64, UnixfsError> {
    let mut written = 0;
    let mut budget = None;
    // An explicit stack instead of recursion: a DAG from the network cannot overflow the call stack.
    let mut pending: Vec<(Cid, Option<u64>)> = vec![(root.clone(), None)];
    while let Some((cid, expected)) = pending.pop() {
        let block = store
            .get_block(&cid)?
            .ok_or_else(|| UnixfsError::MissingBlock(cid.to_string_base32()))?;
        let node = file_block(&cid, block)?;
        let invalid = |reason| UnixfsError::InvalidDag {
            cid: cid.to_string_base32(),
            reason,
        };
        match expected {
            None => {
                if node.declared > MAX_EXPORT_BYTES {
                    return Err(UnixfsError::FileTooLarge {
                        declared: node.declared,
                        limit: MAX_EXPORT_BYTES,
                    });
                }
                budget = Some(node.declared);
            }
            Some(0) => return Err(invalid("child declared with zero size")),
            Some(size) if size != node.declared => {
                return Err(invalid("block size differs from the parent's blocksizes"));
            }
            Some(_) => {}
        }
        if node.blocksizes.len() != node.links.len() {
            return Err(invalid("blocksizes and links differ in count"));
        }
        let children = node
            .blocksizes
            .iter()
            .try_fold(node.data.len() as u64, |sum, size| sum.checked_add(*size));
        if children != Some(node.declared) {
            return Err(invalid("declared size is not data plus children"));
        }
        output.write_all(&node.data)?;
        written += node.data.len() as u64;
        if Some(written) > budget {
            return Err(invalid("file is longer than the root declares"));
        }
        pending.extend(
            node.links
                .into_iter()
                .zip(node.blocksizes.into_iter().map(Some))
                .rev(),
        );
    }
    Ok(written)
}

/// All DAG blocks (root first, no repeats); an error when one is missing locally.
pub fn dag_cids<D: Datastore>(store: &Blockstore<D>, root: &Cid) -> Result<Vec<Cid>, UnixfsError> {
    let mut seen = HashSet::new();
    let mut cids = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(cid) = pending.pop() {
        if !seen.insert(cid.to_bytes()) {
            continue;
        }
        let block = store
            .get_block(&cid)?
            .ok_or_else(|| UnixfsError::MissingBlock(cid.to_string_base32()))?;
        pending.extend(block_links(&cid, &block)?.into_iter().rev());
        cids.push(cid);
    }
    Ok(cids)
}

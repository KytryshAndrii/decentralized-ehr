use crate::error::UnixfsError;
use crate::models::{DataType, PbNode, UnixfsData};
use ipfs_cid::consts::{CODEC_DAG_PB, CODEC_RAW};
use ipfs_cid::models::Cid;
use ipfs_store::{Blockstore, Datastore};
use prost::Message;
use std::collections::HashSet;
use std::io::Write;

/// CID bloków, do których linkuje dany blok. Blok raw i nieznane kodeki nie mają linków.
pub fn block_links(cid: &Cid, block: &[u8]) -> Result<Vec<Cid>, UnixfsError> {
    if cid.codec != CODEC_DAG_PB {
        return Ok(Vec::new());
    }
    PbNode::decode(block)?.link_cids()
}

/// Dane pliku zapisane w samym bloku oraz jego dzieci w kolejności treści.
fn file_block(cid: &Cid, block: Vec<u8>) -> Result<(Vec<u8>, Vec<Cid>), UnixfsError> {
    let unsupported = |reason| UnixfsError::Unsupported {
        cid: cid.to_string_base32(),
        reason,
    };
    match cid.codec {
        CODEC_RAW => Ok((block, Vec::new())),
        CODEC_DAG_PB => {
            let node = PbNode::decode(block.as_slice())?;
            let data = node
                .data
                .as_deref()
                .ok_or(unsupported("dag-pb node without UnixFS data"))?;
            let unixfs = UnixfsData::decode(data)?;
            match unixfs.r#type() {
                DataType::File | DataType::Raw => {
                    Ok((unixfs.data.unwrap_or_default(), node.link_cids()?))
                }
                _ => Err(unsupported("only UnixFS files are supported")),
            }
        }
        _ => Err(unsupported("codec is neither raw nor dag-pb")),
    }
}

/// Odtwarza plik z DAG i zapisuje go do `output`; zwraca liczbę bajtów.
/// Czyta w głąb w kolejności linków, więc obsługuje też DAG trickle i liście dag-pb z Kubo.
/// Każdy blok jest weryfikowany względem CID przez `Blockstore::get_block`.
pub fn cat<D: Datastore>(
    store: &Blockstore<D>,
    root: &Cid,
    output: &mut impl Write,
) -> Result<u64, UnixfsError> {
    let mut written = 0;
    // Jawny stos zamiast rekurencji: głębokość DAG z sieci nie może przepełnić stosu wywołań.
    let mut pending = vec![root.clone()];
    while let Some(cid) = pending.pop() {
        let block = store
            .get_block(&cid)?
            .ok_or_else(|| UnixfsError::MissingBlock(cid.to_string_base32()))?;
        let (data, links) = file_block(&cid, block)?;
        output.write_all(&data)?;
        written += data.len() as u64;
        pending.extend(links.into_iter().rev());
    }
    Ok(written)
}

/// Wszystkie bloki DAG (korzeń pierwszy, bez powtórzeń); błąd, gdy któregoś brakuje lokalnie.
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

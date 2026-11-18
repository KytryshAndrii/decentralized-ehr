use crate::consts::MAX_EXPORT_BYTES;
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

/// Zawartość jednego bloku pliku: własne dane, dzieci oraz rozmiary deklarowane w UnixFS.
struct FileBlock {
    data: Vec<u8>,
    links: Vec<Cid>,
    /// Rozmiar całej treści pod tym blokiem (własne dane + dzieci).
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

/// Odtwarza plik z DAG i zapisuje go do `output`; zwraca liczbę bajtów.
/// Czyta w głąb w kolejności linków, więc obsługuje też DAG trickle i liście dag-pb z Kubo.
/// Każdy blok jest weryfikowany względem CID przez `Blockstore::get_block`.
///
/// Ochrona przed spreparowanym DAG (np. 174 linki do tego samego bloku na każdym poziomie):
/// rozmiar z korzenia jest limitem (i nie może przekraczać `MAX_EXPORT_BYTES`), każde dziecko
/// musi mieć niezerowy rozmiar zgodny z `blocksizes` rodzica, a suma rozmiarów dzieci i własnych
/// danych musi równać się deklarowanemu rozmiarowi węzła. Przekroczenie przerywa odczyt, zanim
/// plik urośnie ponad deklarację.
pub fn cat<D: Datastore>(
    store: &Blockstore<D>,
    root: &Cid,
    output: &mut impl Write,
) -> Result<u64, UnixfsError> {
    let mut written = 0;
    let mut budget = None;
    // Jawny stos zamiast rekurencji: głębokość DAG z sieci nie może przepełnić stosu wywołań.
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

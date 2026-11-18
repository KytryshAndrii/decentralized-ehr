use crate::consts::MAX_FILE_LINKS;
use crate::error::UnixfsError;
use crate::models::{Child, Chunker, ImportedFile, PbNode};
use ipfs_cid::consts::{CODEC_DAG_PB, CODEC_RAW, SHA2_256};
use ipfs_cid::models::Cid;
use ipfs_store::{Blockstore, Datastore};
use std::io::Read;

/// Importuje plik jako UnixFS v1 w układzie zgodnym z
/// `ipfs add --cid-version=1 --raw-leaves --chunker=<chunker>`: liście raw, węzły dag-pb,
/// DAG zrównoważony (balanced). Plik mieszczący się w jednym kawałku to jeden blok raw.
pub fn add_file<D: Datastore>(
    store: &Blockstore<D>,
    reader: impl Read,
    chunker: Chunker,
) -> Result<ImportedFile, UnixfsError> {
    let mut import = Import {
        store,
        new_blocks: 0,
        reused_blocks: 0,
    };
    let mut level = Vec::new();
    for chunk in chunker.split(reader)? {
        let chunk = chunk?;
        level.push(import.put(CODEC_RAW, &chunk, chunk.len() as u64, 0)?);
    }
    if level.is_empty() {
        level.push(import.put(CODEC_RAW, &[], 0, 0)?);
    }
    // Układ balanced: każdy poziom grupuje poprzedni po 174, aż zostanie jeden korzeń.
    // Wszystkie liście są na tej samej głębokości, jak w Kubo.
    while level.len() > 1 {
        let mut parents = Vec::with_capacity(level.len().div_ceil(MAX_FILE_LINKS));
        for children in level.chunks(MAX_FILE_LINKS) {
            let node = PbNode::file(children).to_canonical_bytes();
            let file_bytes = children.iter().map(|child| child.file_bytes).sum();
            let dag_bytes = children.iter().map(|child| child.dag_bytes).sum();
            parents.push(import.put(CODEC_DAG_PB, &node, file_bytes, dag_bytes)?);
        }
        level = parents;
    }
    let root = level.remove(0);
    Ok(ImportedFile {
        root: root.cid,
        size: root.file_bytes,
        new_blocks: import.new_blocks,
        reused_blocks: import.reused_blocks,
    })
}

struct Import<'a, D: Datastore> {
    store: &'a Blockstore<D>,
    new_blocks: usize,
    reused_blocks: usize,
}

impl<D: Datastore> Import<'_, D> {
    /// Zapisuje blok, chyba że Blockstore już go ma: ten sam kawałek w innym pliku
    /// albo w nowej wersji pliku nie zajmuje miejsca drugi raz.
    fn put(
        &mut self,
        codec: u64,
        block: &[u8],
        file_bytes: u64,
        children_dag_bytes: u64,
    ) -> Result<Child, UnixfsError> {
        let cid = Cid::generate_from_data(block, codec, SHA2_256)?;
        if self.store.has(&cid)? {
            self.reused_blocks += 1;
        } else {
            self.store.put_block(&cid, block)?;
            self.new_blocks += 1;
        }
        Ok(Child {
            cid,
            file_bytes,
            dag_bytes: block.len() as u64 + children_dag_bytes,
        })
    }
}

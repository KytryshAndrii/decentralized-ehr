use ipfs_cid::{
    consts::{CODEC_RAW, SHA2_256},
    models::Cid,
};
use ipfs_store::{PersistentBlockstore, StoreError, consts::MAX_BLOCK_BYTES};
use ipfs_unixfs::{Chunker, ImportedFile};
use std::{error::Error, fs, path::Path};

pub type ServiceResult<T> = Result<T, Box<dyn Error>>;

pub struct IpfsService {
    blockstore: PersistentBlockstore,
}

impl IpfsService {
    pub fn new(directory: &Path) -> ServiceResult<Self> {
        Ok(Self {
            blockstore: PersistentBlockstore::open_in(directory)?,
        })
    }

    /// Wyznacza CID bez zapisywania pliku w bazie.
    pub fn create_cid(file_path: &Path) -> ServiceResult<Cid> {
        let bytes = read_block_file(file_path)?;
        Ok(Cid::generate_from_data(&bytes, CODEC_RAW, SHA2_256)?)
    }

    /// Sam Blockstore oblicza CID i zapisuje dane.
    pub fn put_file(&self, file_path: &Path) -> ServiceResult<Cid> {
        Ok(self.blockstore.put(&read_block_file(file_path)?)?)
    }

    /// Import pliku dowolnej wielkości jako UnixFS (stałe kawałki 256 KiB jak domyślnie w Kubo).
    pub fn add_file(&self, file_path: &Path) -> ServiceResult<(ImportedFile, Chunker)> {
        if !file_path.is_absolute() {
            return Err("Input file path must be absolute".into());
        }
        let chunker = Chunker::fixed();
        let file = fs::File::open(file_path)?;
        Ok((
            ipfs_unixfs::add_file(&self.blockstore, file, chunker)?,
            chunker,
        ))
    }

    /// Wszystkie bloki pliku (albo sam blok raw); błąd, gdy któregoś brakuje lokalnie.
    pub fn dag_cids(&self, cid: &Cid) -> ServiceResult<Vec<Cid>> {
        Ok(ipfs_unixfs::dag_cids(&self.blockstore, cid)?)
    }

    pub fn parse_cid(text: &str) -> ServiceResult<Cid> {
        Ok(Cid::decode_from_string(text)?)
    }

    pub fn has_block(&self, cid: &Cid) -> ServiceResult<bool> {
        Ok(self.blockstore.has(cid)?)
    }

    /// Eksport pliku (blok raw albo DAG UnixFS) do absolutnej ścieżki; brak nadpisywania.
    pub fn export_file(&self, cid: &Cid, output_path: &Path) -> ServiceResult<bool> {
        if !output_path.is_absolute() {
            return Err("Output path must be absolute".into());
        }
        if !self.has_block(cid)? {
            return Ok(false);
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_path)?;
        if let Err(error) = ipfs_unixfs::cat(&self.blockstore, cid, &mut output) {
            drop(output);
            let _ = fs::remove_file(output_path);
            return Err(error.into());
        }
        Ok(true)
    }
}

/// Bajty pliku jako jeden blok. Rozmiar jest sprawdzany z metadanych,
/// żeby nie wczytywać do RAM pliku, który i tak nie zmieści się w bloku.
fn read_block_file(path: &Path) -> ServiceResult<Vec<u8>> {
    if !path.is_absolute() {
        return Err("Input file path must be absolute".into());
    }
    if !path.is_file() {
        return Err(format!("Input file does not exist: {}", path.display()).into());
    }
    let size = fs::metadata(path)?.len();
    if size > MAX_BLOCK_BYTES as u64 {
        return Err(StoreError::BlockTooLarge {
            actual: usize::try_from(size).unwrap_or(usize::MAX),
            limit: MAX_BLOCK_BYTES,
        }
        .into());
    }
    Ok(fs::read(path)?)
}

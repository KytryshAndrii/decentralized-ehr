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

    /// Computes the CID without storing the file.
    pub fn create_cid(file_path: &Path) -> ServiceResult<Cid> {
        let bytes = read_block_file(file_path)?;
        Ok(Cid::generate_from_data(&bytes, CODEC_RAW, SHA2_256)?)
    }

    /// The Blockstore computes the CID and stores the data.
    pub fn put_file(&self, file_path: &Path) -> ServiceResult<Cid> {
        Ok(self.blockstore.put(&read_block_file(file_path)?)?)
    }

    /// Imports a file of any size as UnixFS (fixed 256 KiB chunks, the Kubo default).
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

    /// All blocks of the file (or the single raw block); an error when one is missing locally.
    pub fn dag_cids(&self, cid: &Cid) -> ServiceResult<Vec<Cid>> {
        Ok(ipfs_unixfs::dag_cids(&self.blockstore, cid)?)
    }

    pub fn parse_cid(text: &str) -> ServiceResult<Cid> {
        Ok(Cid::decode_from_string(text)?)
    }

    pub fn has_block(&self, cid: &Cid) -> ServiceResult<bool> {
        Ok(self.blockstore.has(cid)?)
    }

    /// Exports the file (raw block or UnixFS DAG) to an absolute path; never overwrites.
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

/// File bytes as one block. The size is checked from the metadata first,
/// so a file that cannot fit in a block is never read into RAM.
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

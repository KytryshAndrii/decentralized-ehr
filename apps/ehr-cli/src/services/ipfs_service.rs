use ipfs_cid::{
    consts::{CODEC_RAW, SHA2_256},
    models::Cid,
};
use ipfs_store::PersistentBlockstore;
use std::{error::Error, fs, io::Write, path::Path};

pub type ServiceResult<T> = Result<T, Box<dyn Error>>;

pub struct IpfsService {
    blockstore: PersistentBlockstore,
}

impl IpfsService {
    pub fn new() -> ServiceResult<Self> {
        Ok(Self {
            blockstore: PersistentBlockstore::open_default()?,
        })
    }

    /// Wyznacza CID bez zapisywania pliku w bazie.
    pub fn create_cid(file_path: &Path) -> ServiceResult<Cid> {
        validate_input_file(file_path)?;
        let bytes = fs::read(file_path)?;
        Ok(Cid::generate_from_data(&bytes, CODEC_RAW, SHA2_256)?)
    }

    /// Sam Blockstore oblicza CID i zapisuje dane.
    pub fn put_file(&self, file_path: &Path) -> ServiceResult<Cid> {
        validate_input_file(file_path)?;
        Ok(self.blockstore.put(&fs::read(file_path)?)?)
    }

    pub fn has_block(&self, cid: &Cid) -> ServiceResult<bool> {
        Ok(self.blockstore.has(cid)?)
    }

    pub fn get_block(&self, cid: &Cid) -> ServiceResult<Option<Vec<u8>>> {
        Ok(self.blockstore.get_block(cid)?)
    }

    /// Eksport bajtów do wskazanej absolutnej ścieżki; brak nadpisywania.
    pub fn export_file(&self, cid: &Cid, output_path: &Path) -> ServiceResult<bool> {
        if !output_path.is_absolute() {
            return Err("Output path must be absolute".into());
        }
        let Some(bytes) = self.get_block(cid)? else {
            return Ok(false);
        };
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_path)?;
        if let Err(error) = output.write_all(&bytes) {
            drop(output);
            let _ = fs::remove_file(output_path);
            return Err(error.into());
        }
        Ok(true)
    }
}

fn validate_input_file(path: &Path) -> ServiceResult<()> {
    if !path.is_absolute() {
        return Err("Input file path must be absolute".into());
    }
    if !path.is_file() {
        return Err(format!("Input file does not exist: {}", path.display()).into());
    }
    Ok(())
}

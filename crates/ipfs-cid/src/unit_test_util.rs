use crate::models::{Cid, Multihash};
use std::fs;

pub fn generate_mock_cid_string(path: &str, codec: u64, hash_algo: u64) -> String {
    let file_bytes = fs::read(path).unwrap_or_else(|_| panic!("Nie znaleziono pliku: {}", path));
    let hash = Multihash::compute(&file_bytes, hash_algo).unwrap();
    let cid = Cid::new_cid(codec, hash);
    cid.to_string_base32()
}

use ipfs_cid::models::Cid;
use std::io::BufReader;

/// Sposób dzielenia pliku na liście DAG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chunker {
    /// Kawałki stałej długości (`size-N` w Kubo); ostatni jest krótszy, bez dopełniania.
    Fixed { size: usize },
    /// Granice zależne od treści (`rabin-min-avg-max` w Kubo).
    Rabin { min: usize, avg: usize, max: usize },
}

/// Iterator po kawałkach pliku; czyta strumieniowo, bez ładowania całego pliku do pamięci.
pub struct Chunks<R> {
    pub(crate) reader: BufReader<R>,
    pub(crate) chunker: Chunker,
    pub(crate) rabin: Option<Box<RabinTables>>,
}

pub(crate) struct RabinTables {
    pub(crate) out: [u64; 256],
    pub(crate) modulo: [u64; 256],
    pub(crate) shift: u32,
    pub(crate) mask: u64,
}

/// Wynik importu pliku.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedFile {
    pub root: Cid,
    /// Rozmiar pliku w bajtach.
    pub size: u64,
    /// Bloki zapisane przy tym imporcie.
    pub new_blocks: usize,
    /// Bloki, które już były w Blockstore (deduplikacja).
    pub reused_blocks: usize,
}

/// Dziecko węzła DAG w trakcie budowy.
pub(crate) struct Child {
    pub(crate) cid: Cid,
    /// Bajty pliku pod tym dzieckiem (`blocksizes`).
    pub(crate) file_bytes: u64,
    /// Łączny rozmiar bloków poddrzewa (`Tsize`).
    pub(crate) dag_bytes: u64,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct PbLink {
    /// Binarny CID dziecka.
    #[prost(bytes = "vec", optional, tag = "1")]
    pub hash: Option<Vec<u8>>,
    #[prost(string, optional, tag = "2")]
    pub name: Option<String>,
    #[prost(uint64, optional, tag = "3")]
    pub tsize: Option<u64>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct PbNode {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub data: Option<Vec<u8>>,
    #[prost(message, repeated, tag = "2")]
    pub links: Vec<PbLink>,
}

/// Komunikat `Data` z UnixFS v1 (proto2); pola mode i mtime są pomijane przy odczycie.
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnixfsData {
    #[prost(enumeration = "DataType", required, tag = "1")]
    pub r#type: i32,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub data: Option<Vec<u8>>,
    #[prost(uint64, optional, tag = "3")]
    pub filesize: Option<u64>,
    #[prost(uint64, repeated, packed = "false", tag = "4")]
    pub blocksizes: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum DataType {
    Raw = 0,
    Directory = 1,
    File = 2,
    Metadata = 3,
    Symlink = 4,
    HamtShard = 5,
}

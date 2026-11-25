use ipfs_cid::models::Cid;
use std::io::BufReader;

/// How a file is split into DAG leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chunker {
    /// Fixed-size chunks (`size-N` in Kubo); the last one is shorter, without padding.
    Fixed { size: usize },
}

/// Iterator over the file chunks; streams without loading the whole file into memory.
pub struct Chunks<R> {
    pub(crate) reader: BufReader<R>,
    pub(crate) chunker: Chunker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedFile {
    pub root: Cid,
    pub size: u64,
    /// Blocks stored by this import.
    pub new_blocks: usize,
    /// Blocks already in the Blockstore (deduplication).
    pub reused_blocks: usize,
}

/// Child of a DAG node being built.
pub(crate) struct Child {
    pub(crate) cid: Cid,
    /// File bytes under this child (`blocksizes`).
    pub(crate) file_bytes: u64,
    /// Total block size of the subtree (`Tsize`).
    pub(crate) dag_bytes: u64,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct PbLink {
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

/// UnixFS v1 `Data` message (proto2); the mode and mtime fields are ignored when reading.
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

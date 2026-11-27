/// Kubo default chunk size (`size-262144`).
pub const DEFAULT_CHUNK_BYTES: usize = 256 * 1024;

/// Maximum number of links in a file node (`UnixFSFileMaxLinks` in Kubo).
pub const MAX_FILE_LINKS: usize = 174;

/// Largest file `cat` rebuilds. The size comes from the DAG root, which is not trusted
/// without a limit: a crafted root could declare terabytes.
pub const MAX_EXPORT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

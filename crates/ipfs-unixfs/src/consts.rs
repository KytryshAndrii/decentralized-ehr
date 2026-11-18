/// Domyślny rozmiar kawałka Kubo (`size-262144`).
pub const DEFAULT_CHUNK_BYTES: usize = 256 * 1024;

/// Maksymalna liczba linków w węźle pliku (`UnixFSFileMaxLinks` w Kubo).
pub const MAX_FILE_LINKS: usize = 174;

/// Największy plik, który `cat` odtworzy. Rozmiar bierze się z korzenia DAG (nie ufamy mu
/// bez limitu: spreparowany korzeń mógłby deklarować terabajty).
pub const MAX_EXPORT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

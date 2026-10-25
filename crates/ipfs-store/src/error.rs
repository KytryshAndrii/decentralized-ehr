use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Błąd LevelDB: {0}")]
    Database(#[from] rusty_leveldb::Status),

    #[error("Błąd systemu plików: {0}")]
    Io(#[from] std::io::Error),

    #[error("Blok nie odpowiada oczekiwanemu CID")]
    IntegrityMismatch,

    #[error("Magazyn zawiera uszkodzone dane pod istniejącym kluczem")]
    CorruptStoredBlock,

    #[error("Nieobsługiwana wersja CID: {0}")]
    UnsupportedCidVersion(u64),

    #[error("Nieobsługiwany algorytm multihash: {0:#x}")]
    UnsupportedHash(u64),

    #[error("Błędna długość digestu dla algorytmu {code:#x}: {actual}, oczekiwano {expected}")]
    InvalidDigestLength {
        code: u64,
        expected: usize,
        actual: usize,
    },

    #[error("Blok przekracza limit ({actual} B > {limit} B)")]
    BlockTooLarge { actual: usize, limit: usize },

    #[error("Błąd synchronizacji dostępu do bazy")]
    LockPoisoned,

    #[error("CID error: {0}")]
    Cid(#[from] ipfs_cid::error::CidError),

    #[error("Blok zmienił się podczas operacji zapisu")]
    ConcurrentModification,
}

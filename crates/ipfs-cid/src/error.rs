use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum CidError {
    #[error("Nieobsugiwana wersja CID: {0}")]
    UnsupportedVersion(u64),

    #[error("Nieobsugiwany multicodec: {0}")]
    UnsupportedMulticodec(u64),

    #[error("Nieobsugiwany algorytm hashujący: {0}")]
    UnsupportedHashAlgorithm(u64),

    #[error("Błąd dekodowania varint: nieprawidłowy format.")]
    VarintDecodeError,

    #[error("CID zwiera dodatkowe, nieoczekiwane bajty na końcu.")]
    TrailingBytes,

    #[error("Bład parsowania multibase: {0}")]
    MultibaseParseError(String),
}

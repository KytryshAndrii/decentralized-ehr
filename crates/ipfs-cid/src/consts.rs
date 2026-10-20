//Kodowanie algorytmów haszujących w formacie multihash

pub const SHA2_256: u64 = 0x12;
pub const SHA2_512: u64 = 0x13;

// Wersja CID oraz kodowanie multicodec dla różnych typów danych
pub const CID_V0: u64 = 0x00;
pub const CID_V1: u64 = 0x01;
pub const CODEC_RAW: u64 = 0x55;
pub const CODEC_DAG_PB: u64 = 0x70;
pub const CODEC_DAG_CBOR: u64 = 0x71;
pub const CODEC_JSON: u64 = 0x0200;

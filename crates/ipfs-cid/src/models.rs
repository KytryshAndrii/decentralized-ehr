#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Multihash {
    pub code: u64,
    pub size: u8,
    pub digest: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Cid {
    pub version: u64,
    pub codec: u64,
    pub multihash: Multihash,
}
